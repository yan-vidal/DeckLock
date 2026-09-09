#!/usr/bin/env python3
"""Guest-only real compositor and PAM boundary assertions, no product test hooks."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

EVIDENCE = Path('/var/tmp/decklock-evidence')
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


def spawn(command, name, extra=None):
    log = (EVIDENCE / name).open('w')
    files.append(log)
    child = subprocess.Popen(command, env=env | (extra or {}), stdout=log, stderr=subprocess.STDOUT)
    processes.append(child)
    return child


def run(command):
    result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=15)
    assert result.returncode == 0, (command, result.returncode, result.stdout, result.stderr)
    return result.stdout


def until(condition, message, seconds=15, child=None):
    deadline = time.monotonic() + seconds
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
    time.sleep(0.5)
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
    assert valid_failures(tally) == 1, ('Real PAM did not record exactly one failed password', tally)
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
    # PAM is authoritative, not the rounded-up 1-minute UI estimate.
    remaining = max(0, 14 - (time.monotonic() - locked_at))
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
    assert client.wait(timeout=10) == -signal.SIGTERM
    type_keys('blocked')
    time.sleep(0.3)
    assert len(text('probe-keys').splitlines()) == count, 'SIGTERM exposed the underlying client'
    assert '.unlock_and_destroy(' not in text('account-denied.log')
    record('SIGTERM leaves real compositor locked and input isolated')
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
