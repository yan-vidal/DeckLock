#!/usr/bin/env python3
"""Guest-only greetd/Cage/DeckLock login and user-selection integration test."""
import os
from pathlib import Path
import pwd
import shutil
import shlex
import subprocess
import time
import tomllib

EVIDENCE = Path('/var/tmp/decklock-evidence')
WORK = Path('/var/tmp/decklock-greeter-test')
# Emulated guests (aarch64 without KVM) run several times slower; test-vm.py
# passes the factor, and every bounded wait below scales with it. 1 under KVM.
SLOW = float(os.environ.get('DECKLOCK_VM_SLOWDOWN', '1'))
UNIT = 'decklock-vm-greetd.service'
assert Path('/etc/decklock-test-vm').read_text().strip() == 'disposable-qemu-fixture'
assert os.geteuid() == 0, 'Only the disposable guest root may start greetd'
assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() in ('kvm', 'qemu')


def run(command, **kwargs):
    return subprocess.run(command, check=True, timeout=30 * SLOW, **kwargs)


def greetd_running():
    return subprocess.run(['systemctl', 'is-active', '--quiet', UNIT]).returncode == 0


def until(condition, label, seconds=45):
    deadline = time.monotonic() + seconds * SLOW
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


config = Path('/etc/greetd/config.toml')
packaged_config = tomllib.loads(config.read_text()) if config.exists() else {}
greeter_name = packaged_config.get('default_session', {}).get('user', 'greeter')
if not subprocess.run(['id', '-u', greeter_name], capture_output=True).returncode == 0:
    run(['useradd', '--system', '--create-home', '--shell', '/bin/sh', greeter_name])
if not subprocess.run(['id', '-u', 'locktest2'], capture_output=True).returncode == 0:
    run(['useradd', '--create-home', '--shell', '/bin/bash', 'locktest2'])
run(['chpasswd'], input='locktest2:DeckLock-second-42\n', text=True)
# Reproduce the Arch system-account boundary on every distribution.
run(['usermod', '--home', '/', greeter_name])
greeter = pwd.getpwnam(greeter_name)
assert greeter.pw_dir == '/'
WORK.mkdir(mode=0o777, exist_ok=True)
WORK.chmod(0o1777)
session_file = Path('/usr/share/wayland-sessions/00-decklock-vm.desktop')
session_file.parent.mkdir(parents=True, exist_ok=True)
session_file.write_text('[Desktop Entry]\nName=DeckLock VM session\nExec=/usr/local/bin/decklock-vm-session\nType=Application\n')
session_command = Path('/usr/local/bin/decklock-vm-session')
session_command.write_text(
    '#!/bin/sh\n'
    'set -eu\n'
    # The test waits for session-<user>, then reads runtime-<user>: write the
    # runtime marker first, and each one whole through a rename.
    'dir=/var/tmp/decklock-greeter-test; me=$(id -un)\n'
    'printf "%s\\n" "${XDG_RUNTIME_DIR:-unset}" > "$dir/.runtime-$me" && mv "$dir/.runtime-$me" "$dir/runtime-$me"\n'
    'printf "%s\\n" "$me" > "$dir/.session-$me" && mv "$dir/.session-$me" "$dir/session-$me"\n'
    'sleep 2\n'
)
session_command.chmod(0o755)

# Inspect the actual setup output before adding guest-only headless variables.
run(['decklock', 'setup', 'greeter'])
document = tomllib.loads(config.read_text())
assert document['default_session']['user'] == greeter_name
original = document['default_session']['command']
assert 'cage -s -- decklock --greeter --keyboard' in original, original
assert 'XDG_CONFIG_HOME=' in original and 'XDG_CACHE_HOME=' in original, original
storage_env = dict(word.split('=', 1) for word in shlex.split(original) if '=' in word)
state_dir = Path(storage_env['XDG_CONFIG_HOME'])
if Path('/sys/fs/selinux/enforce').exists():
    assert state_dir == Path('/var/lib/greetd/decklock')
    assert subprocess.check_output(['stat', '-c', '%C', str(state_dir)], text=True).split(':')[2] == 'xdm_var_lib_t'
for path in [state_dir, state_dir / 'cache', state_dir / 'data']:
    info = path.stat()
    assert info.st_uid == greeter.pw_uid and info.st_gid == greeter.pw_gid, path
    assert info.st_mode & 0o777 == 0o750, path
# Ownership of the leaf is insufficient when a packaged parent is private to
# another account. Probe actual traversal and writing as the configured account.
run(['runuser', '-u', greeter_name, '--', 'python3', '-c',
     'from pathlib import Path; import sys; p=Path(sys.argv[1])/".setup-write-check"; '
     'p.write_text("probe"); p.unlink()', str(state_dir)])
record('configured packaged greeter account can traverse and write its storage')
state = state_dir / 'greeter-state.toml'
state.write_text('last_user = "locktest"\nlast_session = "00-decklock-vm"\n')
os.chown(state, greeter.pw_uid, greeter.pw_gid)
record('packaged system setup prepares private storage owned by the HOME=/ greeter')

greeter_config = state_dir / 'decklock/config.toml'
greeter_config.parent.mkdir(parents=True, exist_ok=True)
greeter_config.write_text('locale = "en-US"\nbackground_pool = []\nidle_pool = []\nidle_enabled = false\n')
run(['chown', '-R', f'{greeter.pw_uid}:{greeter.pw_gid}', str(greeter_config.parent)])
wrapper = Path('/usr/local/bin/decklock-vm-greeter')
wrapper.write_text(
    '#!/bin/sh\n'
    'set -eu\n'
    'exec >> /var/tmp/decklock-greeter-test/cage.log 2>&1\n'
    'id\n'
    'printf "runtime=%s\\n" "${XDG_RUNTIME_DIR:-unset}"\n'
    'printf "home=%s\\n" "$HOME"\n'
    'printf "start\\n" >> /var/tmp/decklock-greeter-test/starts\n'
    'exec env WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_RENDERER=pixman '
    'WLR_LIBINPUT_NO_DEVICES=1 GDK_BACKEND=wayland GSK_RENDERER=cairo '
    f'{original}\n'
)
wrapper.chmod(0o755)
active_config = Path('/etc/greetd/decklock-vm.toml')
active_config.write_text(
    '[terminal]\nvt = 2\nswitch = true\n'
    '[general]\nrunfile = "/run/decklock-vm-greetd.run"\n'
    '[default_session]\ncommand = "' + str(wrapper) + '"\nuser = "' + greeter_name + '"\n'
)
WORK.chmod(0o1777)
if Path('/usr/sbin/restorecon').exists() or Path('/sbin/restorecon').exists():
    run(['restorecon', '-RF', '/etc/greetd', str(wrapper), str(session_file),
         str(session_command), str(state_dir)])
run(['loginctl', 'enable-linger', greeter_name])
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
    assert 'home=/' in (WORK / 'cage.log').read_text().splitlines()
    record('greetd starts the exact setup command with HOME=/ and its logind runtime directory')
    def type_password(value):
        greeter_env = os.environ.copy()
        greeter_env.update(
            HOME=greeter.pw_dir, XDG_RUNTIME_DIR=str(runtime),
            WAYLAND_DISPLAY=socket.name,
        )
        run(['runuser', '-u', greeter_name, '--', 'env',
             f'HOME={greeter.pw_dir}', f'XDG_RUNTIME_DIR={runtime}',
             f'WAYLAND_DISPLAY={socket.name}',
             'wtype', '-s', '250', '-d', '30', value], env=greeter_env)
        run(['runuser', '-u', greeter_name, '--', 'env',
             f'HOME={greeter.pw_dir}', f'XDG_RUNTIME_DIR={runtime}',
             f'WAYLAND_DISPLAY={socket.name}',
             'wtype', '-s', '250', '-k', 'Return'], env=greeter_env)

    time.sleep(2 * SLOW)
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
    time.sleep(2 * SLOW)
    # The saved selection is locktest. The installed image may contain
    # another regular account, so derive a bounded number of avatar steps
    # from the guest's passwd order rather than assuming two accounts.
    users = [entry.pw_name for entry in pwd.getpwall()
             if 1000 <= entry.pw_uid < 60000
             and not entry.pw_shell.endswith(('nologin', 'false'))]
    steps = (users.index('locktest2') - users.index('locktest')) % len(users)
    assert steps > 0
    for _ in range(steps):
        run(['runuser', '-u', greeter_name, '--', 'env',
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
