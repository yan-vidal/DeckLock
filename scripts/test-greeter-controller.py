#!/usr/bin/env python3
"""Real CLI/GTK with a fake sc-controller, only on the gate's private display."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
assert 'decklock-check-' in os.environ.get('XDG_RUNTIME_DIR', ''), 'Use scripts/check --all'
assert os.environ.get('DISPLAY') and os.environ.get('XAUTHORITY'), 'Private Xvfb required'
assert not os.environ.get('WAYLAND_DISPLAY') and not os.environ.get('WAYLAND_SOCKET')


def wait_for(predicate, message, timeout=8):
    end = time.monotonic() + timeout
    while not predicate():
        assert time.monotonic() < end, message
        time.sleep(0.01)


def dead(pid):
    try:
        status = Path(f'/proc/{pid}/stat').read_text()
        return status[status.rindex(')') + 2:].split()[0] == 'Z'
    except FileNotFoundError:
        return True


FAKE = '''#!/usr/bin/python3
import json, os, pathlib, signal, socket, sys, time
root = pathlib.Path(os.environ['FAKE_STATE'])
config = pathlib.Path(os.environ['XDG_CONFIG_HOME'])
(root/'started.tmp').write_text(json.dumps({'pid':os.getpid(), 'args':sys.argv[1:], 'home':os.environ['HOME'], 'config':str(config), 'profile':json.loads(pathlib.Path(sys.argv[1]).read_text())}))
(root/'started.tmp').replace(root/'started.json')
if os.environ.get('FAKE_IGNORE_TERM'): signal.signal(signal.SIGTERM, signal.SIG_IGN)
if os.environ.get('FAKE_FAILURE'): raise SystemExit(1)
if os.environ.get('FAKE_HANG'):
    while True: time.sleep(1)
listener = socket.socket(socket.AF_UNIX)
listener.bind(str(config/'scc/daemon.socket'))
listener.listen()
while True:
    peer, _ = listener.accept()
    with peer:
        peer.sendall(b'Controller Count: 0\\nReady.\\n' if os.environ.get('FAKE_NO_DEVICE') else b'Controller: greeter-test deck 0 None\\nController Count: 1\\nReady.\\n')
        with peer.makefile('rb') as lines:
            for line in lines:
                with (root/'commands').open('ab') as log: log.write(line)
                if line.startswith((b'Controller:', b'Lock:')): peer.sendall(b'OK.\\n')
                if line.startswith(b'Lock:') and os.environ.get('FAKE_HIDE'):
                    peer.sendall(b'Event: greeter-test B 1\\n')
'''


def run_case(parent, name, arguments, termination=None, failure=False, hang=False,
             hide=False, capture=True, greetd=True, ignore_term=False, missing=False, no_device=False):
    state = parent / name
    state.mkdir()
    env = dict(os.environ, HOME=str(state), XDG_CONFIG_HOME=str(state/'config'),
               XDG_DATA_HOME=str(state/'data'), XDG_CACHE_HOME=str(state/'cache'),
               GDK_BACKEND='x11', GTK_A11Y='none', FAKE_STATE=str(state),
               GREETD_SOCK=str(state/'fake-greetd.socket'),
               PATH=str(parent/'bin') + ':' + os.environ['PATH'])
    env.pop('DECKLOCK_GREETER_APPEARANCE', None)
    if not greetd: env.pop('GREETD_SOCK')
    if missing: env['PATH'] = str(parent/'empty-bin')
    if failure: env['FAKE_FAILURE'] = '1'
    if hang: env['FAKE_HANG'] = '1'
    if hide: env['FAKE_HIDE'] = '1'
    if ignore_term: env['FAKE_IGNORE_TERM'] = '1'
    if no_device: env['FAKE_NO_DEVICE'] = '1'
    log = (state/'process.log').open('w')
    child = subprocess.Popen([str(ROOT/'target/debug/decklock'), *arguments], env=env,
                             stdout=log, stderr=subprocess.STDOUT)
    daemon_pid = None
    try:
        started = state/'started.json'
        if termination:
            wait_for(started.exists, f'{name}: daemon not started')
            info = json.loads(started.read_text())
            daemon_pid = info['pid']
            assert Path(info['home']) != state
            assert Path(info['config']).is_relative_to(Path(env['XDG_RUNTIME_DIR']))
            assert info['args'][1:] == ['start', '--foreground', '--alone'], info
            assert info['profile']['buttons']['A']['action'] == 'button(Keys.KEY_ENTER)'
            commands = state/'commands'
            wait_for(lambda: commands.exists() and b'Lock:' in commands.read_bytes(),
                     f'{name}: keyboard never captured the fake controller')
            child.send_signal(termination)
        child.wait(timeout=8)
        log.flush()
        if termination:
            assert child.returncode == -termination, (name, child.returncode)
        else:
            assert child.returncode == 0, (name, (state/'process.log').read_text())
        if started.exists():
            info = json.loads(started.read_text())
            daemon_pid = info['pid']
            wait_for(lambda: dead(daemon_pid), f'{name}: orphan controller daemon')
            if not termination:
                assert not Path(info['home']).exists(), f'{name}: private files were not removed'
        if missing:
            assert not started.exists(), f'{name}: unexpected daemon'
            assert 'Greeter controller unavailable:' in (state/'process.log').read_text()
        elif '--preview' in arguments or '--greeter-controller' not in arguments or not greetd:
            assert not started.exists(), f'{name}: preview/default greeter launched a daemon'
        else:
            assert started.exists(), f'{name}: daemon was not attempted'
            if failure or hang:
                assert 'Greeter controller unavailable:' in (state/'process.log').read_text()
            elif not termination:
                commands = (state/'commands').read_bytes() if (state/'commands').exists() else b''
                assert (b'Lock:' in commands) == capture, f'{name}: capture visibility contract failed'
                if hide:
                    assert commands.count(b'Lock:') == 1 and b'Unlock.' in commands, commands
        assert not (state/'config/scc').exists(), f'{name}: modified saved controller configuration'
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=3)
        if daemon_pid and not dead(daemon_pid):
            os.kill(daemon_pid, signal.SIGKILL)
        log.close()


with tempfile.TemporaryDirectory(prefix='decklock-greeter-controller-') as temporary:
    parent = Path(temporary)
    (parent/'bin').mkdir()
    daemon = parent/'bin/scc-daemon'
    daemon.write_text(FAKE)
    daemon.chmod(0o755)
    wm = subprocess.Popen(['xfwm4', '--compositor=off'], stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL)
    try:
        common = ['--greeter', '--keyboard', '--preview-exit-after', '2']
        run_case(parent, 'default', common)
        run_case(parent, 'preview', [*common, '--greeter-controller', '--preview'])
        run_case(parent, 'no-greetd', [*common, '--greeter-controller'], greetd=False)
        run_case(parent, 'normal', [*common, '--greeter-controller'])
        run_case(parent, 'hidden', [*common, '--greeter-controller'], hide=True)
        run_case(parent, 'hidden-at-start', ['--greeter', '--greeter-controller', '--preview-exit-after', '2'], capture=False)
        run_case(parent, 'no-device', [*common, '--greeter-controller'], no_device=True, capture=False)
        run_case(parent, 'stalled-shutdown', [*common, '--greeter-controller'], ignore_term=True)
        run_case(parent, 'failure', [*common, '--greeter-controller'], failure=True)
        run_case(parent, 'missing', [*common, '--greeter-controller'], missing=True)
        run_case(parent, 'timeout', [*common, '--greeter-controller'], hang=True)
        for sig in [signal.SIGTERM, signal.SIGKILL]:
            run_case(parent, sig.name, ['--greeter', '--keyboard', '--greeter-controller'], termination=sig)
    finally:
        wm.terminate()
        wm.wait(timeout=3)
print('PASS: greeter-only daemon isolation, capture, fallback, normal exit, SIGTERM and SIGKILL')
