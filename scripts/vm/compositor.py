#!/usr/bin/env python3
"""Guest-only lock boundary on a further compositor, with real PAM.

exercise.py covers the PAM policy cases (faillock, account denial) and X11 once,
on Sway. What differs between compositors is the ext-session-lock boundary
itself, so this repeats only that part: input isolation, a real PAM denial, one
unlock, input returning afterwards, and a killed locker leaving the session
locked. No product test hooks; the compositor is started unmodified.
"""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

EVIDENCE = Path('/var/tmp/decklock-evidence')
# Emulated guests (aarch64 without KVM) run several times slower; test-vm.py
# passes the factor, and every bounded wait below scales with it. 1 under KVM.
SLOW = float(os.environ.get('DECKLOCK_VM_SLOWDOWN', '1'))
assert Path('/etc/decklock-test-vm').read_text().strip() == 'disposable-qemu-fixture'
assert os.getuid() != 0, 'Run the locker as an ordinary guest user'
# The guest has no GPU. labwc renders with pixman. Wayfire renders only with
# GLES, and wlroots insists on a DRM render node for it, which the emulated
# display card lacks. setup.sh loads vgem, a render node with no hardware
# behind it; Mesa keeps buffers in memory there and draws them with llvmpipe,
# which wlroots accepts only when software rendering is explicitly allowed.
SOFTWARE_GLES = 'software-gles'
COMPOSITORS = {
    'labwc': {'command': ['labwc'], 'renderer': 'pixman'},
    'wayfire': {
        'command': ['wayfire', '-c', 'wayfire.ini'],
        'renderer': SOFTWARE_GLES,
        # Only the lock protocol plugin: no effects, and no configuration from
        # the guest user's home.
        'config': ('wayfire.ini', '[core]\nplugins = session-lock\nxwayland = false\n'),
    },
    # Hyprland is not here: Aquamarine opens a seat and a DRM device and needs a
    # GBM allocator from a real GPU even for headless outputs. Tried on vgem with
    # HYPRLAND_HEADLESS_ONLY, it still stopped with "no allocator available".
}
NAME = sys.argv[1] if len(sys.argv) == 2 else ''
assert NAME in COMPOSITORS, f'Usage: compositor.py {{{",".join(COMPOSITORS)}}}'
OUT = EVIDENCE / f'compositor-{NAME}'
OUT.mkdir(exist_ok=True)
os.chdir(OUT)
env = os.environ.copy()
for key in ['DISPLAY', 'WAYLAND_DISPLAY', 'WAYLAND_SOCKET', 'SWAYSOCK']:
    env.pop(key, None)
env.update(WLR_BACKENDS='headless', WLR_HEADLESS_OUTPUTS='1', WLR_RENDERER='pixman',
           WLR_LIBINPUT_NO_DEVICES='1', GDK_BACKEND='wayland', GSK_RENDERER='cairo', GTK_A11Y='atspi',
           LC_ALL='C.UTF-8', DECKLOCK_PROBE_DIR=str(OUT))
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi

processes = []
files = []
results = []


def spawn(command, name, extra=None):
    log = (OUT / name).open('w')
    files.append(log)
    child = subprocess.Popen(command, env=env | (extra or {}), stdout=log, stderr=subprocess.STDOUT)
    processes.append(child)
    return child


def run(command):
    result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=15 * SLOW)
    assert result.returncode == 0, (command, result.returncode, result.stdout, result.stderr)
    return result.stdout


def until(condition, message, seconds=15, child=None):
    deadline = time.monotonic() + seconds * SLOW
    while time.monotonic() < deadline:
        if child is not None and child.poll() is not None:
            raise AssertionError(f'{NAME}: {message}: child exited {child.returncode}')
        if condition():
            return
        time.sleep(0.05)
    raise AssertionError(f'{NAME}: timed out: {message}')


def text(name):
    path = OUT / name
    return path.read_text(errors='replace') if path.exists() else ''


def keys():
    return len(text('probe-keys').splitlines())


def statuses():
    found = []
    def visit(node, depth=0):
        if node is None or depth > 25:
            return
        name = node.get_name() or ''
        if name.startswith(('Authentication failed', 'Account locked', 'Authenticating')):
            found.append(name)
        for index in range(node.get_child_count()):
            visit(node.get_child_at_index(index), depth + 1)
    desktop = Atspi.get_desktop(0)
    for index in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(index)
        if app.get_name() == 'decklock':
            visit(app)
    return found


def record(name):
    results.append(f'{NAME}: {name}')
    (OUT / 'assertions.json').write_text(json.dumps(results, indent=2) + '\n')
    print(f'PASS: {NAME}: {name}', flush=True)


def type_keys(value):
    run(['wtype', '-s', '250', '-d', '30', value])


def start_lock(name, compositor):
    child = spawn(['decklock', '--lock', '--config', str(OUT / 'config.toml')], name,
                  {'WAYLAND_DEBUG': 'client'})
    until(lambda: '.locked(' in text(name), 'compositor confirmed lock', child=child)
    until(lambda: '.get_lock_surface(' in text(name), 'lock surface created', child=child)
    assert compositor.poll() is None, f'{NAME} exited while locking'
    time.sleep(0.5 * SLOW)
    return child


def submit(value):
    run(['wtype', '-s', '250', '-M', 'ctrl', '-k', 'a', '-m', 'ctrl', '-k', 'BackSpace'])
    type_keys(value)
    run(['wtype', '-s', '250', '-k', 'Return'])


try:
    runtime = Path(env['XDG_RUNTIME_DIR'])
    def sockets():
        return {p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')}
    existing = sockets()
    spec = COMPOSITORS[NAME]
    extra = {}
    if spec['renderer'] == SOFTWARE_GLES:
        # vgem is a bare platform device named vgem, with no driver bound to it.
        found = {node.name: (node / 'device').resolve().name
                 for node in sorted(Path('/sys/class/drm').glob('renderD*'))}
        nodes = [Path('/dev/dri') / name for name, device in found.items() if device == 'vgem']
        assert nodes, f'No vgem render node for software GLES (setup.sh loads vgem): {found}'
        extra = {'WLR_RENDERER': 'gles2', 'WLR_RENDER_DRM_DEVICE': str(nodes[0]),
                 'WLR_RENDERER_ALLOW_SOFTWARE': '1', 'GBM_ALWAYS_SOFTWARE': '1'}
    if 'config' in spec:
        (OUT / spec['config'][0]).write_text(spec['config'][1])
    compositor = spawn(spec['command'], 'compositor.log', extra)
    until(lambda: sockets() - existing, 'Wayland socket ready', child=compositor)
    env['WAYLAND_DISPLAY'] = sorted(sockets() - existing)[0]
    (OUT / 'config.toml').write_text('locale = "en-US"\nbackground_pool = []\nidle_pool = []\nidle_enabled = false\n')
    probe = spawn(['python3', str(EVIDENCE / 'probe.py')], 'probe.log')
    until(lambda: (OUT / 'probe-mapped').exists(), 'probe mapped', child=probe)
    time.sleep(0.5 * SLOW)
    type_keys('before')
    until(lambda: keys() == 6, 'probe receives input before lock', child=compositor)
    assert list(map(int, text('probe-keys').splitlines())) == list(map(ord, 'before'))
    record('ordinary Wayland client receives input before lock')

    client = start_lock('lock.log', compositor)
    count = keys()
    submit('wrong-fixture-password')
    until(lambda: any(x.startswith('Authentication failed') for x in statuses()),
          'real PAM denial displayed', seconds=35, child=client)
    assert client.poll() is None, 'Wrong password exited the locker'
    assert '.unlock_and_destroy(' not in text('lock.log'), 'Wrong password requested unlock'
    assert keys() == count, 'Input escaped to the underlying client'
    record('wrong password denied by real PAM; underlying client receives no input')

    submit('DeckLock-test-42')
    until(lambda: client.poll() is not None, 'correct password unlocks and exits', seconds=35)
    assert client.returncode == 0, text('lock.log')[-3000:]
    assert text('lock.log').count('.unlock_and_destroy(') == 1, 'Expected one real protocol unlock'
    record(f'correct password passes real PAM and releases the {NAME} lock exactly once')
    type_keys('after')
    until(lambda: keys() == count + 5, 'input resumes after unlock', child=compositor)
    assert list(map(int, text('probe-keys').splitlines())) == list(map(ord, 'beforeafter'))
    record('ordinary client receives input again after unlock')

    client = start_lock('sigterm.log', compositor)
    count = keys()
    client.send_signal(signal.SIGTERM)
    assert client.wait(timeout=10 * SLOW) == -signal.SIGTERM
    type_keys('blocked')
    time.sleep(0.3 * SLOW)
    assert keys() == count, 'SIGTERM exposed the underlying client'
    assert compositor.poll() is None, f'{NAME} exited after the locker was killed'
    record(f'SIGTERM leaves {NAME} locked and input isolated')
except BaseException:
    # guest.log is what the job prints; the compositor's own words go with it.
    print(f'--- {NAME} compositor.log (last 40 lines) ---', *text('compositor.log').splitlines()[-40:],
          sep='\n', flush=True)
    raise
finally:
    for child in reversed(processes):
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
    for file in files:
        file.close()
