#!/usr/bin/env python3
"""Guest-only greetd/Cage/DeckLock login and user-selection integration test."""
import os
from pathlib import Path
import pwd
import shutil
import shlex
import subprocess
import struct
import zlib
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

# Exercise the combined command with sudo's root HOME and a fixed guest user.
# The locker file, its directories and backups must remain writable by that user.
caller = pwd.getpwnam('locktest')
lock_dir = Path(caller.pw_dir) / '.config/hypr'
lock_dir.mkdir(parents=True, exist_ok=True)
lock_file = lock_dir / 'hypridle.conf'
lock_file.write_text('general { lock_cmd = swaylock }\n')
for path in [lock_dir.parent, lock_dir, lock_file]:
    os.chown(path, caller.pw_uid, caller.pw_gid)
lock_file.chmod(0o600)
# Personal media must remain inaccessible from the service account HOME.
private = Path(caller.pw_dir) / '.config/decklock'
private.mkdir(mode=0o700, exist_ok=True)
private.chmod(0o700)
personal_photo = private / 'private.png'
def png_chunk(kind, data):
    return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
raw = (b'\x00' + b'\x00\xff\xff' * 16) * 16
personal_photo.write_bytes(b'\x89PNG\r\n\x1a\n' +
    png_chunk(b'IHDR', struct.pack('!2I5B', 16, 16, 8, 2, 0, 0, 0)) +
    png_chunk(b'IDAT', zlib.compress(raw)) + png_chunk(b'IEND', b''))
personal_video = private / 'private.mp4'
packaged_video = next(Path('/usr/share/decklock/media').rglob('*.mp4'))
shutil.copyfile(packaged_video, personal_video)
personal_config = private / 'config.toml'
personal_config.write_text('locale="en-US"\nbackground_pool=["private.png"]\nidle_pool=["private.mp4"]\nidle_enabled=false\n')
for path in [private, personal_photo, personal_video, personal_config]:
    os.chown(path, caller.pw_uid, caller.pw_gid)
for path in [personal_photo, personal_video, personal_config]:
    path.chmod(0o600)
root_config = WORK / 'root-config'
setup_env = dict(os.environ, HOME='/root', XDG_CONFIG_HOME=str(root_config),
                 SUDO_UID=str(caller.pw_uid), SUDO_GID=str(caller.pw_gid), SUDO_USER='locktest')
saved_lock = lock_file.read_bytes()
saved_greetd = config.read_bytes() if config.exists() else None
run(['decklock', 'setup', 'all', '--dry-run'], env=setup_env)
assert lock_file.read_bytes() == saved_lock
assert (config.read_bytes() if config.exists() else None) == saved_greetd
assert not root_config.exists(), 'Combined dry-run touched root configuration'
run(['decklock', 'setup', 'all'], env=setup_env)
assert 'lock_cmd = decklock --lock' in lock_file.read_text()
assert lock_file.stat().st_uid == caller.pw_uid
assert lock_file.stat().st_gid == caller.pw_gid
assert lock_file.stat().st_mode & 0o777 == 0o600
backups = list(lock_dir.glob('hypridle.conf.bak.*'))
assert backups and all(p.stat().st_uid == caller.pw_uid for p in backups)
assert backups[0].read_bytes() == saved_lock
assert not root_config.exists(), 'Combined setup wrote the locker to root configuration'
after_lock = lock_file.read_bytes()
run(['decklock', 'setup', 'all'], env=setup_env)
assert lock_file.read_bytes() == after_lock
assert list(lock_dir.glob('hypridle.conf.bak.*')) == backups
record('combined packaged setup uses the sudo caller HOME, ownership, mode and backups; dry-run and repeat preserve files')

# Inspect the actual setup output before adding guest-only headless variables.
document = tomllib.loads(config.read_text())
assert document['default_session']['user'] == greeter_name
original = document['default_session']['command']
assert 'cage -s -- decklock --greeter --keyboard' in original, original
assert 'XDG_CONFIG_HOME=' in original and 'XDG_CACHE_HOME=' in original, original
storage_env = dict(word.split('=', 1) for word in shlex.split(original) if '=' in word)
state_dir = Path(storage_env['XDG_CONFIG_HOME'])
appearance_path = Path(storage_env['DECKLOCK_GREETER_APPEARANCE'])
appearance = tomllib.loads(appearance_path.read_text())
normal_media = Path(appearance['background_pool'][0])
idle_media = Path(appearance['idle_pool'][0])
assert normal_media.read_bytes() == personal_photo.read_bytes()
assert idle_media.read_bytes() == personal_video.read_bytes()
for path in [appearance_path, normal_media, idle_media]:
    assert path.is_relative_to(state_dir), path
    assert path.stat().st_uid == greeter.pw_uid and path.stat().st_gid == greeter.pw_gid
    assert path.stat().st_mode & 0o777 == 0o640
    run(['setpriv', '--reuid', str(greeter.pw_uid), '--regid', str(greeter.pw_gid),
         '--init-groups', '--', '/usr/bin/test', '-r', str(path)])
denied = subprocess.run(['setpriv', '--reuid', str(greeter.pw_uid), '--regid', str(greeter.pw_gid),
                         '--init-groups', '--', '/usr/bin/test', '-r', str(personal_photo)], timeout=10 * SLOW)
assert denied.returncode != 0, 'Setup opened the private user directory to the greeter'
assert private.stat().st_mode & 0o777 == 0o700
assert personal_photo.stat().st_mode & 0o777 == 0o600
record('packaged setup copies normal/rest media for the login account without granting access to private HOME files')
# A root installer must never copy a source that the desktop caller cannot read.
root_only_media = WORK / 'root-only.png'
root_only_media.write_bytes(personal_photo.read_bytes())
root_only_media.chmod(0o600)
saved_personal = personal_config.read_bytes()
saved_appearance = appearance_path.read_bytes()
saved_greetd_media = config.read_bytes()
personal_config.write_text(f'background_pool=["{root_only_media}"]\n')
rejected = subprocess.run(['decklock', 'setup', 'greeter'], env=setup_env,
                          capture_output=True, text=True, timeout=30 * SLOW)
assert rejected.returncode != 0, 'Root setup bypassed the desktop caller media permissions'
assert 'Media export failed' in rejected.stderr, rejected.stderr
assert appearance_path.read_bytes() == saved_appearance
assert config.read_bytes() == saved_greetd_media
personal_config.write_bytes(saved_personal)
root_only_media.unlink()
record('root installation rejects unreadable caller media before changing valid login/appearance')
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
# A correct child owner/label must not hide an inaccessible ancestor.
private_parent = state_dir / '.private-parent'
private_parent.mkdir(mode=0o700)
blocked_storage = private_parent / 'state'
saved_config = config.read_bytes()
blocked = subprocess.run(
    ['decklock', 'setup', 'greeter', '--state-dir', str(blocked_storage)],
    capture_output=True, text=True, timeout=30 * SLOW)
assert blocked.returncode != 0, 'Setup accepted storage beneath a root-only ancestor'
assert 'cannot access greeter storage' in blocked.stderr, blocked.stderr
assert config.read_bytes() == saved_config, 'Rejected storage changed the working greetd command'
record('setup rejects inaccessible ancestors before replacing the greetd command')
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
    def copied_background_visible():
        output = WORK / 'greeter-media.ppm'
        run(['runuser', '-u', greeter_name, '--', 'env', f'XDG_RUNTIME_DIR={runtime}',
             f'WAYLAND_DISPLAY={socket.name}', 'grim', '-t', 'ppm', str(output)])
        with output.open('rb') as image:
            assert image.readline().strip() == b'P6'
            line = image.readline()
            while line.startswith(b'#'):
                line = image.readline()
            width, height = map(int, line.split())
            assert image.readline().strip() == b'255'
            pixels = image.read()
        assert len(pixels) == width * height * 3
        # Far left of the controls: cyan photo + black veil keeps red near zero.
        offset = ((height // 2) * width + 8) * 3
        red, green, blue = pixels[offset:offset+3]
        return red < 5 and green > 80 and blue > 80

    until(copied_background_visible, 'installed private photo rendered by the real greeter', 30)
    record('real Cage greeter renders copied private media with HOME=/; original user files remain unreadable')
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
