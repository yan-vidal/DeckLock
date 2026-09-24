#!/usr/bin/env python3
"""Guest-only real compositor and PAM boundary assertions, no product test hooks."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

EVIDENCE = Path('/var/tmp/decklock-evidence')
# Emulated guests (aarch64 without KVM) run several times slower; test-vm.py
# passes the factor, and every bounded wait below scales with it. 1 under KVM.
SLOW = float(os.environ.get('DECKLOCK_VM_SLOWDOWN', '1'))
assert Path('/etc/decklock-test-vm').read_text().strip() == 'disposable-qemu-fixture'
assert os.getuid() != 0, 'Run the locker as an ordinary guest user'
os.chdir(EVIDENCE)
env = os.environ.copy()
for key in ['DISPLAY', 'WAYLAND_DISPLAY', 'WAYLAND_SOCKET', 'SWAYSOCK']:
    env.pop(key, None)
env.update(WLR_BACKENDS='headless', WLR_HEADLESS_OUTPUTS='1', WLR_RENDERER='pixman',
           WLR_LIBINPUT_NO_DEVICES='1', GDK_BACKEND='wayland', GSK_RENDERER='cairo', GTK_A11Y='atspi',
           LC_ALL='C.UTF-8')
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi

processes = []
files = []
results = []


def spawn(command, name, extra=None, full_env=None):
    log = (EVIDENCE / name).open('w')
    files.append(log)
    process_env = full_env if full_env is not None else (env | (extra or {}))
    child = subprocess.Popen(command, env=process_env, stdout=log, stderr=subprocess.STDOUT)
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
            raise AssertionError(f'{message}: child exited {child.returncode}')
        if condition():
            return
        time.sleep(0.05)
    raise AssertionError('Timed out: ' + message)


def text(name):
    path = EVIDENCE / name
    return path.read_text(errors='replace') if path.exists() else ''


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
    results.append(name)
    (EVIDENCE / 'assertions.json').write_text(json.dumps(results, indent=2) + '\n')
    print('PASS: ' + name, flush=True)


def valid_failures(tally):
    return sum(line.split()[-1:] == ['V'] for line in tally.splitlines())


def type_keys(value):
    run(['wtype', '-s', '250', '-d', '30', value])


def start_lock(name):
    child = spawn(['decklock', '--lock', '--config', str(EVIDENCE / 'config.toml')], name,
                  {'WAYLAND_DEBUG': 'client'})
    until(lambda: '.locked(' in text(name), 'compositor confirmed lock', child=child)
    until(lambda: '.get_lock_surface(' in text(name), 'lock surface created', child=child)
    time.sleep(0.5 * SLOW)
    return child


def submit(value):
    # Clear any previous text through the ordinary keyboard input path.
    run(['wtype', '-s', '250', '-M', 'ctrl', '-k', 'a', '-m', 'ctrl', '-k', 'BackSpace'])
    type_keys(value)
    run(['wtype', '-s', '250', '-k', 'Return'])


try:
    (EVIDENCE / 'sway.conf').write_text('output HEADLESS-1 mode 1280x720\nseat seat0 fallback true\nxwayland disable\n')
    sway = spawn(['sway', '--unsupported-gpu', '-d', '-c', str(EVIDENCE / 'sway.conf')], 'sway.log')
    runtime = Path(env['XDG_RUNTIME_DIR'])
    until(lambda: list(runtime.glob('sway-ipc.*.sock')), 'Sway IPC ready', child=sway)
    env['SWAYSOCK'] = str(next(runtime.glob('sway-ipc.*.sock')))
    until(lambda: list(runtime.glob('wayland-*')), 'Wayland socket ready', child=sway)
    env['WAYLAND_DISPLAY'] = next(p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock'))
    probe = spawn(['python3', str(EVIDENCE / 'probe.py')], 'probe.log')
    until(lambda: 'DeckLock input probe' in run(['swaymsg', '-t', 'get_tree']), 'probe mapped', child=probe)
    type_keys('before')
    until(lambda: len(text('probe-keys').splitlines()) == 6, 'probe receives input before lock')
    assert list(map(int, text('probe-keys').splitlines())) == list(map(ord, 'before'))
    record('ordinary Wayland client receives input before lock')
    (EVIDENCE / 'config.toml').write_text('locale = "en-US"\nbackground_pool = []\nidle_pool = []\nidle_enabled = false\n')
    client = start_lock('lock.log')
    count = len(text('probe-keys').splitlines())
    submit('wrong-fixture-password')
    until(lambda: any(x.startswith('Authentication failed') for x in statuses()),
          'real PAM denial displayed', seconds=35, child=client)
    assert client.poll() is None, 'Wrong password exited the locker'
    assert '.unlock_and_destroy(' not in text('lock.log'), 'Wrong password requested unlock'
    assert len(text('probe-keys').splitlines()) == count, 'Input escaped to the underlying client'
    tally = run(['faillock', '--user', 'locktest'])
    (EVIDENCE / 'failed-tally.txt').write_text(tally)
    # Counting failures is the default stack's policy, read from configuration by
    # setup.sh; missing is a harness error, so there is deliberately no default.
    expected = int(os.environ['DECKLOCK_STACK_FAILLOCK'])
    assert valid_failures(tally) == expected, (
        f'Real PAM recorded {valid_failures(tally)} failed password(s); the default stack expects {expected}', tally)
    record('wrong password denied by real PAM; underlying client receives no input')
    submit('DeckLock-test-42')
    until(lambda: client.poll() is not None, 'correct password unlocks and exits', seconds=35)
    assert client.returncode == 0, text('lock.log')[-3000:]
    assert text('lock.log').count('.unlock_and_destroy(') == 1, 'Expected one real protocol unlock'
    type_keys('after')
    until(lambda: len(text('probe-keys').splitlines()) == count + 5, 'input resumes after unlock')
    record('correct password passes real PAM and releases Sway lock exactly once')
    assert list(map(int, text('probe-keys').splitlines())) == list(map(ord, 'beforeafter'))
    record('ordinary client receives input again after unlock')
    with (EVIDENCE / 'config.toml').open('a') as config:
        config.write('pam_service = "decklock-vm-test"\n')
    client = start_lock('faillock.log')
    for attempt in range(2):
        submit('wrong-fixture-password')
        until(lambda: 'Authenticating' not in ' '.join(statuses()) and bool(statuses()),
              'failure response', seconds=35, child=client)
        tally = run(['faillock', '--user', 'locktest'])
        until(lambda: valid_failures(run(['faillock', '--user', 'locktest'])) == attempt + 1,
              'failure tallied by pam_faillock', seconds=35, child=client)
    locked_at = time.monotonic()
    submit('DeckLock-test-42')
    until(lambda: any(x.startswith('Account locked') for x in statuses()),
          'PAM lockout notice displayed', seconds=10, child=client)
    notice = next(x for x in statuses() if x.startswith('Account locked'))
    assert 'Estimated wait:' in notice, notice
    (EVIDENCE / 'lockout-notice.txt').write_text(notice + '\n')
    assert '.unlock_and_destroy(' not in text('faillock.log'), 'Locked account was accepted'
    record('real pam_faillock rejects correct password during lockout and supplies visible estimate')
    until(lambda: any(x.startswith('Account locked') and x != notice for x in statuses()),
          'countdown advances', seconds=4, child=client)
    record('PAM countdown updates on the real lock screen')
    # PAM is authoritative, not the UI estimate rounded up to whole minutes.
    # setup.sh sets unlock_time to 12 s times the slowdown.
    remaining = max(0, (12 + 2) * SLOW - (time.monotonic() - locked_at))
    time.sleep(remaining)
    submit('DeckLock-test-42')
    until(lambda: client.poll() is not None, 'PAM permits auth after its own timeout', seconds=35)
    assert client.returncode == 0
    assert text('faillock.log').count('.unlock_and_destroy(') == 1
    record('PAM expiry permits unlock before the rounded UI countdown reaches zero')
    config_path = EVIDENCE / 'config.toml'
    config_path.write_text(config_path.read_text().replace('decklock-vm-test', 'decklock-vm-account'))
    client = start_lock('account-denied.log')
    count = len(text('probe-keys').splitlines())
    submit('DeckLock-test-42')
    until(lambda: any(x.startswith('Authentication failed') for x in statuses()),
          'account policy denies the correct password', seconds=35, child=client)
    assert '.unlock_and_destroy(' not in text('account-denied.log')
    assert len(text('probe-keys').splitlines()) == count
    record('real PAM account policy prevents unlock even with the correct password')
    # This final lock also supplies the fail-closed termination case.

    count = len(text('probe-keys').splitlines())
    client.send_signal(signal.SIGTERM)
    assert client.wait(timeout=10 * SLOW) == -signal.SIGTERM
    type_keys('blocked')
    time.sleep(0.3 * SLOW)
    assert len(text('probe-keys').splitlines()) == count, 'SIGTERM exposed the underlying client'
    record('SIGTERM leaves real compositor locked and input isolated')

    # Part 2: Real PAM authentication under X11 backend
    import shutil
    if shutil.which('Xvfb') and shutil.which('xdotool'):
        x11_env = env.copy()
        for key in ['DISPLAY', 'WAYLAND_DISPLAY', 'WAYLAND_SOCKET', 'SWAYSOCK']:
            x11_env.pop(key, None)
        x11_env.update(DISPLAY=':99', GDK_BACKEND='x11', GSK_RENDERER='cairo')

        xvfb = spawn(['Xvfb', ':99', '-screen', '0', '1280x720x24', '-nolisten', 'tcp'], 'xvfb.log', full_env=x11_env)
        until(lambda: Path('/tmp/.X11-unix/X99').exists(), 'Xvfb :99 socket ready', child=xvfb)

        def type_x11(value):
            subprocess.run(['xdotool', 'type', '--delay', '40', value], env=x11_env, check=True, timeout=10 * SLOW)

        def submit_x11(value):
            subprocess.run(['xdotool', 'key', 'ctrl+a', 'BackSpace'], env=x11_env, check=True, timeout=10 * SLOW)
            type_x11(value)
            subprocess.run(['xdotool', 'key', 'Return'], env=x11_env, check=True, timeout=10 * SLOW)

        # On X11 the lock is the keyboard and pointer grabs, and DeckLock refuses
        # every password until it holds both. A fixed delay before typing lost the
        # keys on an emulated guest, so this asks the server as another client
        # would: a grab someone else holds is refused with AlreadyGrabbed. A grab
        # this probe does get is released at once.
        import ctypes
        import ctypes.util
        xlib = ctypes.CDLL(ctypes.util.find_library('X11'))
        xlib.XOpenDisplay.restype = ctypes.c_void_p
        xlib.XOpenDisplay.argtypes = [ctypes.c_char_p]
        xlib.XDefaultRootWindow.restype = ctypes.c_ulong
        xlib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        xlib.XGrabKeyboard.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
                                       ctypes.c_int, ctypes.c_ulong]
        xlib.XGrabPointer.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_uint,
                                      ctypes.c_int, ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
        for name in ['XUngrabKeyboard', 'XUngrabPointer']:
            getattr(xlib, name).argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]
        GRAB_SUCCESS, ALREADY_GRABBED, GRAB_ASYNC = 0, 1, 1

        def x11_grabs_held():
            display = xlib.XOpenDisplay(b':99')
            assert display, 'Could not open Xvfb :99 to probe the grabs'
            try:
                root = xlib.XDefaultRootWindow(display)
                keyboard = xlib.XGrabKeyboard(display, root, 0, GRAB_ASYNC, GRAB_ASYNC, 0)
                pointer = xlib.XGrabPointer(display, root, 0, 0, GRAB_ASYNC, GRAB_ASYNC, 0, 0, 0)
                if keyboard == GRAB_SUCCESS:
                    xlib.XUngrabKeyboard(display, 0)
                if pointer == GRAB_SUCCESS:
                    xlib.XUngrabPointer(display, 0)
                return keyboard == ALREADY_GRABBED and pointer == ALREADY_GRABBED
            finally:
                xlib.XCloseDisplay(display)

        (EVIDENCE / 'config-x11.toml').write_text('locale = "en-US"\npam_service = "decklock-vm-test"\nbackground_pool = []\nidle_pool = []\nidle_enabled = false\n')
        xclient = spawn(['decklock', '--lock', '--config', str(EVIDENCE / 'config-x11.toml')], 'lock-x11.log', full_env=x11_env)
        until(x11_grabs_held, 'X11 locker holds the keyboard and pointer', child=xclient)

        # 1. Real PAM denial on X11
        submit_x11('wrong-fixture-password')
        until(lambda: any(x.startswith('Authentication failed') for x in statuses()),
              'real PAM denial displayed on X11', seconds=35, child=xclient)
        assert xclient.poll() is None, 'Wrong password exited X11 locker'
        record('wrong password denied by real PAM under X11 backend')

        # 2. Real PAM authorization and unlock on X11
        submit_x11('DeckLock-test-42')
        until(lambda: xclient.poll() is not None, 'correct password unlocks and exits under X11', seconds=35)
        assert xclient.returncode == 0, text('lock-x11.log')[-3000:]
        record('correct password passes real PAM and unlocks X11 session')

        # 3. SIGTERM on X11 releases the session (asserting the reduced guarantee)
        xclient2 = spawn(['decklock', '--lock', '--config', str(EVIDENCE / 'config-x11.toml')], 'lock-x11-sigterm.log', full_env=x11_env)
        until(x11_grabs_held, 'second X11 locker holds the keyboard and pointer', child=xclient2)
        xclient2.send_signal(signal.SIGTERM)
        assert xclient2.wait(timeout=10 * SLOW) == -signal.SIGTERM
        until(lambda: not x11_grabs_held(), 'grabs released once the X11 locker is killed', seconds=5)
        record('SIGTERM under X11 releases the session as Guarantees declares')

        xvfb.terminate()
        try:
            xvfb.wait(timeout=5)
        except subprocess.TimeoutExpired:
            xvfb.kill()
            xvfb.wait()
        if xvfb in processes:
            processes.remove(xvfb)
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
