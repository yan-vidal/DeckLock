#!/usr/bin/env python3
"""Boot a disposable QEMU guest and test the installed package with real Sway/PAM.

One target per distribution: the image is pinned by name and SHA256, and the guest
installs that distribution's own candidate package. Sway and wtype are packaged at
the same versions on every target, so the assertions in vm/exercise.py are shared;
only provisioning varies, through vm/distros/<target>.env.

Never mounts the host home, devices, desktop sockets or credentials into the guest.
Requires QEMU/KVM, qemu-img, genisoimage, curl, OpenSSH and 3 GiB available RAM.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
DISTROS = {
    'arch': {
        'image': 'Arch-Linux-x86_64-cloudimg-20260901.583572.qcow2',
        'sha256': 'e3e688f97a71b265ce202905a504253f60f3680cf57d011a45411c43bedfa930',
        'base': 'https://geo.mirror.pkgbuild.com/images/v20260901.583572/',
        'suffix': '.pkg.tar.zst',
    },
    'fedora': {
        'image': 'Fedora-Cloud-Base-Generic-43-1.6.x86_64.qcow2',
        'sha256': '846574c8a97cd2d8dc1f231062d73107cc85cbbbda56335e264a46e3a6c8ab2f',
        'base': 'https://dl.fedoraproject.org/pub/fedora/linux/releases/43/Cloud/x86_64/images/',
        'suffix': '.rpm',
    },
    'ubuntu': {
        'image': 'ubuntu-26.04-server-cloudimg-amd64.img',
        'sha256': '4908fb59ccd4e87ae4e8e973b7ef56f535448eacb24a87fd787270c0048987bc',
        'base': 'https://cloud-images.ubuntu.com/releases/26.04/release/',
        'suffix': '.deb',
    },
}
# qemu-img 2.x is still on some PATHs ahead of the system copy, for instance from
# a bundled Android SDK. Refuse it rather than creating an overlay with a tool
# that old and failing later in a way that looks like a guest problem.
MINIMUM_QEMU_IMG = (6, 0)


def qemu_img():
    path = shutil.which('qemu-img')
    if not path:
        raise SystemExit('Missing dependency: qemu-img')
    reported = subprocess.check_output([path, '--version'], text=True).split()
    version = next((field for field in reported if field[:1].isdigit()), '0')
    parsed = tuple(int(part) for part in re.findall(r'\d+', version)[:2])
    if parsed < MINIMUM_QEMU_IMG:
        raise SystemExit(
            f'qemu-img {version} at {path} is older than '
            f'{".".join(map(str, MINIMUM_QEMU_IMG))}; put the system copy first on PATH')
    return path


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def available_mib():
    return memory_stats()['host_available_mib']


def memory_stats():
    values = {key: int(value.split()[0]) // 1024 for key, value in
              (line.split(':', 1) for line in Path('/proc/meminfo').read_text().splitlines())}
    pressure = Path('/proc/pressure/memory').read_text().splitlines()
    full = next(line for line in pressure if line.startswith('full '))
    avg10 = float(dict(field.split('=') for field in full.split()[1:])['avg10'])
    return {'host_available_mib': values['MemAvailable'],
            'host_swap_used_mib': values['SwapTotal'] - values['SwapFree'],
            'host_memory_full_avg10': avg10}


def main():
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f'Interrupted by signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True, help='Candidate package built by CI')
    parser.add_argument('--target', choices=sorted(DISTROS), default='arch')
    parser.add_argument('--logs', type=Path, default=ROOT / 'target/vm-logs')
    args = parser.parse_args()
    distro = DISTROS[args.target]
    package = args.package.resolve(strict=True)
    if not package.name.endswith(distro['suffix']):
        raise SystemExit(f'{args.target} expects a {distro["suffix"]} candidate, got {package.name}')
    logs = args.logs.resolve()
    logs.mkdir(parents=True, exist_ok=True)
    for name in ['qemu-system-x86_64', 'genisoimage', 'curl', 'ssh', 'scp', 'ssh-keygen']:
        if not shutil.which(name):
            raise SystemExit(f'Missing dependency: {name}')
    image_tool = qemu_img()
    if not os.access('/dev/kvm', os.R_OK | os.W_OK):
        raise SystemExit('KVM required: refusing slow software emulation on the shared machine')
    if available_mib() < 3072:
        raise SystemExit('Need at least 3072 MiB available before starting the 2048 MiB VM')
    cache = ROOT / '.deps/vm-cache'
    cache.mkdir(parents=True, exist_ok=True)
    base = cache / distro['image']
    if not base.exists():
        partial = base.with_name(base.name + '.part')
        if not partial.exists() or digest(partial) != distro['sha256']:
            subprocess.run(['curl', '-fL', '--retry', '2', '--max-time', '600', '-o', str(partial),
                            distro['base'] + distro['image']], check=True)
        if digest(partial) != distro['sha256']:
            raise SystemExit('Image SHA256 mismatch')
        partial.rename(base)
    if digest(base) != distro['sha256']:
        raise SystemExit('Cached image SHA256 mismatch')
    report = {'test_source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'target': args.target, 'image': distro['image'], 'image_sha256': distro['sha256'],
              'package_sha256': digest(package), 'package': package.name,
              'memory_mib': 2048, 'cpus': 2, 'status': 'running'}
    (logs / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    # The copy-on-write overlay grows with every guest write, and a package
    # install writes hundreds of MiB. /tmp is tmpfs by default on Arch and Fedora,
    # where that growth would be host RAM rather than disk, so the work directory
    # sits beside the image cache instead of in the system temporary directory.
    with tempfile.TemporaryDirectory(prefix='decklock-vm-', dir=cache) as directory:
        work = Path(directory)
        key = work / 'id_ed25519'
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(key)], check=True)
        seed = work / 'seed'
        seed.mkdir()
        public = key.with_suffix('.pub').read_text().strip()
        (seed / 'meta-data').write_text('instance-id: decklock-isolated-test\nlocal-hostname: decklock-test-vm\n')
        (seed / 'user-data').write_text('#cloud-config\ndisable_root: false\nssh_pwauth: false\nusers:\n  - name: root\n    ssh_authorized_keys:\n      - ' + public + '\nwrite_files:\n  - path: /etc/decklock-test-vm\n    content: disposable-qemu-fixture\nruncmd:\n  - [systemctl, enable, --now, sshd]\n')
        subprocess.run(['genisoimage', '-quiet', '-output', str(work / 'seed.iso'), '-volid', 'cidata', '-joliet', '-rock', str(seed)], check=True)
        subprocess.run([image_tool, 'create', '-q', '-f', 'qcow2', '-F', 'qcow2', '-b', str(base), str(work / 'disk.qcow2'), '12G'], check=True)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        command = ['qemu-system-x86_64', '-name', 'decklock-isolated-test', '-enable-kvm', '-cpu', 'host',
                   '-m', '2048', '-smp', '2', '-display', 'none', '-monitor', 'none', '-no-reboot',
                   '-drive', f'file={work}/disk.qcow2,if=virtio,format=qcow2',
                   '-drive', f'file={work}/seed.iso,media=cdrom,readonly=on',
                   '-netdev', f'user,id=net0,hostfwd=tcp:127.0.0.1:{port}-:22',
                   '-device', 'virtio-net-pci,netdev=net0', '-device', 'virtio-rng-pci',
                   '-serial', f'file:{logs}/serial.log']
        ssh_options = ['-i', str(key), '-o', 'IdentitiesOnly=yes', '-o', 'BatchMode=yes',
                       '-o', 'StrictHostKeyChecking=accept-new', '-o', f'UserKnownHostsFile={work}/known_hosts',
                       '-o', 'ConnectTimeout=3', '-o', 'LogLevel=ERROR']
        ssh = ['ssh', *ssh_options, '-p', str(port), 'root@127.0.0.1']
        stopped = threading.Event()
        def guard(vm):
            with (logs / 'memory.jsonl').open('w') as log:
                low = 0
                while not stopped.wait(3):
                    sample = memory_stats()
                    log.write(json.dumps({'time': time.time(), **sample}) + '\n')
                    log.flush()
                    low = low + 1 if sample['host_available_mib'] < 768 or sample['host_memory_full_avg10'] > 5 else 0
                    if low >= 3:
                        report['memory_guard'] = 'Stopped owned VM: low host memory or sustained memory pressure for 9 seconds'
                        vm.terminate()
                        return
        with (logs / 'qemu.log').open('w') as output:
            vm = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT)
            thread = threading.Thread(target=guard, args=(vm,), daemon=True)
            thread.start()
            try:
                print(f'VM started: pid={vm.pid}, KVM, 2048 MiB, 2 CPUs. Logs: {logs}', flush=True)
                deadline = time.monotonic() + 240
                while True:
                    if vm.poll() is not None:
                        raise RuntimeError('VM exited before SSH; see serial.log/qemu.log')
                    ready = subprocess.run([*ssh, 'test -f /etc/decklock-test-vm'], capture_output=True, timeout=8)
                    if ready.returncode == 0:
                        break
                    if time.monotonic() > deadline:
                        raise TimeoutError('VM boot/SSH deadline exceeded')
                    time.sleep(2)
                subprocess.run([*ssh, 'mkdir -p /root/decklock-fixture /var/tmp/decklock-evidence'], check=True, timeout=15)
                fixture = sorted(p for p in (ROOT / 'scripts/vm').iterdir() if p.suffix in {'.py', '.sh'})
                fixture.append(ROOT / 'scripts/vm/distros' / f'{args.target}.env')
                subprocess.run(['scp', *ssh_options, '-P', str(port), str(package), *map(str, fixture), 'root@127.0.0.1:/root/decklock-fixture/'], check=True, timeout=60)
                print('Guest ready; installing dependencies and exercising the candidate.', flush=True)
                with (logs / 'guest.log').open('w') as guest_log:
                    result = subprocess.run([*ssh, f'bash /root/decklock-fixture/setup.sh {args.target}'], stdout=guest_log, stderr=subprocess.STDOUT, timeout=1200)
                if result.returncode:
                    raise RuntimeError(f'Guest test failed ({result.returncode}); see guest.log')
                report['status'] = 'passed'
            except BaseException as error:
                report['status'] = 'failed'
                report['error'] = str(error)
                raise
            finally:
                if vm.poll() is None:
                    try:
                        transfer = subprocess.run(['scp', *ssh_options, '-P', str(port), '-r', 'root@127.0.0.1:/var/tmp/decklock-evidence', str(logs)], timeout=30, check=False)
                        if transfer.returncode:
                            report['evidence_error'] = f'Guest evidence transfer failed ({transfer.returncode})'
                    except subprocess.TimeoutExpired:
                        report['evidence_error'] = 'Guest evidence transfer timed out'
                if 'evidence_error' in report:
                    report['status'] = 'failed'
                stopped.set()
                vm.terminate()
                try:
                    vm.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    vm.kill()
                    vm.wait()
                thread.join(timeout=5)
                (logs / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
                # The job log is often the only evidence a reviewer can read, so a
                # failure names its assertion there instead of only in guest.log.
                guest = logs / 'guest.log'
                if report['status'] != 'passed' and guest.exists():
                    tail = guest.read_text(errors='replace').splitlines()[-80:]
                    print('--- guest.log (last 80 lines) ---', *tail, '--- end guest.log ---',
                          sep='\n', file=sys.stderr, flush=True)
    if report['status'] != 'passed':
        raise SystemExit('FAIL: VM evidence incomplete')
    print(f'PASS: real Sway/PAM package VM on {args.target}', flush=True)


if __name__ == '__main__':
    main()
