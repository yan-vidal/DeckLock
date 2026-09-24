#!/usr/bin/env python3
"""Guest-only greetd/Cage/DeckLock login and user-selection integration test."""
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import time
import tomllib

EVIDENCE = Path('/var/tmp/decklock-evidence')
WORK = Path('/var/tmp/decklock-greeter-test')
UNIT = 'decklock-vm-greetd.service'
assert Path('/etc/decklock-test-vm').read_text().strip() == 'disposable-qemu-fixture'
assert os.geteuid() == 0, 'Only the disposable guest root may start greetd'
assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() in ('kvm', 'qemu')


def run(command, **kwargs):
    return subprocess.run(command, check=True, timeout=30, **kwargs)


def greetd_running():
    return subprocess.run(['systemctl', 'is-active', '--quiet', UNIT]).returncode == 0


def until(condition, label, seconds=45):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if not greetd_running():
            raise AssertionError(f'{label}: greetd exited; see greetd.log')
        result = condition()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError(f'Timed out: {label}; see greetd.log')


def record(name):
    with (EVIDENCE / 'greeter-assertions.txt').open('a') as output:
        output.write('PASS: ' + name + '\n')
    print('PASS: ' + name, flush=True)


if not subprocess.run(['id', '-u', 'greeter'], capture_output=True).returncode == 0:
    run(['useradd', '--system', '--create-home', '--shell', '/bin/sh', 'greeter'])
if not subprocess.run(['id', '-u', 'locktest2'], capture_output=True).returncode == 0:
    run(['useradd', '--create-home', '--shell', '/bin/bash', 'locktest2'])
run(['chpasswd'], input='locktest2:DeckLock-second-42\n', text=True)
greeter = pwd.getpwnam('greeter')
WORK.mkdir(mode=0o777, exist_ok=True)
WORK.chmod(0o1777)
state = Path(greeter.pw_dir) / '.local/state/decklock/greeter-state.toml'
state.parent.mkdir(parents=True, exist_ok=True)
state.write_text('last_user = "locktest"\nlast_session = "00-decklock-vm"\n')
run(['chown', '-R', 'greeter:greeter', str(state.parent.parent.parent)])

session_file = Path('/usr/share/wayland-sessions/00-decklock-vm.desktop')
session_file.parent.mkdir(parents=True, exist_ok=True)
session_file.write_text('[Desktop Entry]\nName=DeckLock VM session\nExec=/usr/local/bin/decklock-vm-session\nType=Application\n')
session_command = Path('/usr/local/bin/decklock-vm-session')
session_command.write_text(
    '#!/bin/sh\n'
    'set -eu\n'
    'printf "%s\\n" "$(id -un)" > "/var/tmp/decklock-greeter-test/session-$(id -un)"\n'
    'printf "%s\\n" "${XDG_RUNTIME_DIR:-unset}" > "/var/tmp/decklock-greeter-test/runtime-$(id -un)"\n'
    'sleep 2\n'
)
session_command.chmod(0o755)

# Inspect the actual setup output before adding guest-only headless variables.
config = WORK / 'config.toml'
run(['decklock', 'setup', 'greeter', '--target', str(config)])
document = tomllib.loads(config.read_text())
original = document['default_session']['command']
assert original == 'cage -s -- decklock --greeter --keyboard', original
record('packaged setup selects the native greetd/Cage greeter')

greeter_config = Path(greeter.pw_dir) / '.config/decklock/config.toml'
greeter_config.parent.mkdir(parents=True, exist_ok=True)
greeter_config.write_text('locale = "en-US"\nbackground_pool = []\nidle_pool = []\nidle_enabled = false\n')
run(['chown', '-R', 'greeter:greeter', str(greeter_config.parent.parent)])
wrapper = Path('/usr/local/bin/decklock-vm-greeter')
wrapper.write_text(
    '#!/bin/sh\n'
    'set -eu\n'
    'exec >> /var/tmp/decklock-greeter-test/cage.log 2>&1\n'
    'id\n'
    'printf "runtime=%s\\n" "${XDG_RUNTIME_DIR:-unset}"\n'
    'printf "start\\n" >> /var/tmp/decklock-greeter-test/starts\n'
    'exec env WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_RENDERER=pixman '
    'WLR_LIBINPUT_NO_DEVICES=1 GDK_BACKEND=wayland GSK_RENDERER=cairo '
    'cage -s -- decklock --greeter --keyboard '
    f'--config {greeter_config}\n'
)
wrapper.chmod(0o755)
active_config = Path('/etc/greetd/decklock-vm.toml')
active_config.write_text(
    '[terminal]\nvt = 2\nswitch = true\n'
    '[general]\nrunfile = "/run/decklock-vm-greetd.run"\n'
    '[default_session]\ncommand = "' + str(wrapper) + '"\nuser = "greeter"\n'
)
WORK.chmod(0o1777)
if Path('/usr/sbin/restorecon').exists() or Path('/sbin/restorecon').exists():
    run(['restorecon', '-RF', '/etc/greetd', str(wrapper), str(session_file),
         str(session_command), greeter.pw_dir])
run(['loginctl', 'enable-linger', 'greeter'])
run(['systemctl', 'stop', 'greetd.service'])
run(['systemctl', 'stop', 'getty@tty2.service'])
runtime = Path('/run/user') / str(greeter.pw_uid)
# greetd must run as a system service, as the distributions' greetd.service
# does. Started from this SSH login it would sit inside that logind session:
# pam_systemd then refuses to register the greeter and user sessions ("already
# running in a session"), and neither receives XDG_RUNTIME_DIR.
subprocess.run(['systemctl', 'reset-failed', UNIT], capture_output=True)
run(['systemd-run', '--unit', UNIT, '--collect', '-p', 'IgnoreSIGPIPE=no', '-p', 'SendSIGHUP=yes',
     '-p', 'KeyringMode=shared', 'greetd', '--config', str(active_config)])
try:
    def greeter_socket():
        if not (WORK / 'starts').exists():
            return None
        return next((path for path in runtime.glob('wayland-*') if path.is_socket()), None)

    socket = until(greeter_socket, 'Cage Wayland socket ready', 60)
    first_socket_inode = socket.stat().st_ino
    greeter_runtime = f'runtime={runtime}'
    assert greeter_runtime in (WORK / 'cage.log').read_text(errors='replace').splitlines(), \
        'greetd did not give the greeter its logind runtime directory; see cage.log'
    record('greetd gives the packaged greeter command its logind runtime directory')
    def type_password(value):
        greeter_env = os.environ.copy()
        greeter_env.update(
            HOME=greeter.pw_dir, XDG_RUNTIME_DIR=str(runtime),
            WAYLAND_DISPLAY=socket.name,
        )
        run(['runuser', '-u', 'greeter', '--', 'env',
             f'HOME={greeter.pw_dir}', f'XDG_RUNTIME_DIR={runtime}',
             f'WAYLAND_DISPLAY={socket.name}',
             'wtype', '-s', '250', '-d', '30', value], env=greeter_env)
        run(['runuser', '-u', 'greeter', '--', 'env',
             f'HOME={greeter.pw_dir}', f'XDG_RUNTIME_DIR={runtime}',
             f'WAYLAND_DISPLAY={socket.name}',
             'wtype', '-s', '250', '-k', 'Return'], env=greeter_env)

    time.sleep(2)
    type_password('wrong-fixture-password')
    until(lambda: 'Greeter login failed:' in (WORK / 'cage.log').read_text(errors='replace'),
          'wrong password visibly rejected', 35)
    assert not (WORK / 'session-locktest').exists(), 'Wrong password started a session'
    record('real greetd/PAM rejects a wrong password without starting a user session')

    type_password('DeckLock-test-42')
    until(lambda: (WORK / 'session-locktest').exists(), 'first user session started', 45)
    assert (WORK / 'session-locktest').read_text().strip() == 'locktest'
    assert (WORK / 'runtime-locktest').read_text().strip() == '/run/user/' + str(pwd.getpwnam('locktest').pw_uid), \
        'user session lacks its logind runtime directory'
    assert tomllib.loads(state.read_text())['last_user'] == 'locktest'
    record('native greeter starts the selected Wayland session as the authenticated user')

    until(lambda: (WORK / 'starts').read_text().count('start') >= 2,
          'greeter restarted after logout', 45)
    socket = until(
        lambda: next((path for path in runtime.glob('wayland-*')
                      if path.is_socket() and path.stat().st_ino != first_socket_inode), None),
        'new Cage Wayland socket ready after logout', 45)
    time.sleep(2)
    # The saved selection is locktest. The installed image may contain
    # another regular account, so derive a bounded number of avatar steps
    # from the guest's passwd order rather than assuming two accounts.
    users = [entry.pw_name for entry in pwd.getpwall()
             if 1000 <= entry.pw_uid < 60000
             and not entry.pw_shell.endswith(('nologin', 'false'))]
    steps = (users.index('locktest2') - users.index('locktest')) % len(users)
    assert steps > 0
    for _ in range(steps):
        run(['runuser', '-u', 'greeter', '--', 'env',
             f'HOME={greeter.pw_dir}', f'XDG_RUNTIME_DIR={runtime}',
             f'WAYLAND_DISPLAY={socket.name}', 'wtype', '-s', '250', '-k', 'Right'])
    type_password('DeckLock-second-42')
    until(lambda: (WORK / 'session-locktest2').exists(), 'second user session started', 45)
    assert (WORK / 'session-locktest2').read_text().strip() == 'locktest2'
    assert (WORK / 'runtime-locktest2').read_text().strip() == '/run/user/' + str(pwd.getpwnam('locktest2').pw_uid), \
        'user session lacks its logind runtime directory'
    assert tomllib.loads(state.read_text())['last_user'] == 'locktest2'
    record('avatar selection changes the account authenticated by real greetd/PAM')
finally:
    if (WORK / 'cage.log').exists():
        shutil.copyfile(WORK / 'cage.log', EVIDENCE / 'cage.log')
    subprocess.run(['systemctl', 'stop', UNIT], capture_output=True, timeout=45)
    with (EVIDENCE / 'greetd.log').open('w') as output:
        subprocess.run(['journalctl', '--no-pager', '-o', 'short-monotonic', '-u', UNIT],
                       stdout=output, stderr=subprocess.STDOUT, timeout=30)
