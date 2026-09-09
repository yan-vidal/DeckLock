#!/usr/bin/env python3
"""Boot disposable Arch/QEMU and test the installed package with real Sway/PAM.

Never mounts the host home, devices, desktop sockets or credentials into the guest.
Requires QEMU/KVM, qemu-img, genisoimage, curl, OpenSSH and 3 GiB available RAM.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
IMAGE = 'Arch-Linux-x86_64-cloudimg-20260901.583572.qcow2'
IMAGE_SHA256 = 'e3e688f97a71b265ce202905a504253f60f3680cf57d011a45411c43bedfa930'
IMAGE_URL = 'https://geo.mirror.pkgbuild.com/images/v20260901.583572/' + IMAGE


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
    parser.add_argument('--package', type=Path, required=True, help='Candidate .pkg.tar.zst built by CI')
    parser.add_argument('--logs', type=Path, default=ROOT / 'target/vm-logs')
    args = parser.parse_args()
    package = args.package.resolve(strict=True)
    logs = args.logs.resolve()
    logs.mkdir(parents=True, exist_ok=True)
    for name in ['qemu-system-x86_64', 'qemu-img', 'genisoimage', 'curl', 'ssh', 'scp', 'ssh-keygen']:
        if not shutil.which(name):
            raise SystemExit(f'Missing dependency: {name}')
    if not os.access('/dev/kvm', os.R_OK | os.W_OK):
        raise SystemExit('KVM required: refusing slow software emulation on the shared machine')
    if available_mib() < 3072:
        raise SystemExit('Need at least 3072 MiB available before starting the 2048 MiB VM')
    cache = ROOT / '.deps/vm-cache'
    cache.mkdir(parents=True, exist_ok=True)
    base = cache / IMAGE
    if not base.exists():
        partial = base.with_suffix('.qcow2.part')
        if not partial.exists() or digest(partial) != IMAGE_SHA256:
            subprocess.run(['curl', '-fL', '--retry', '2', '--max-time', '600', '-o', str(partial), IMAGE_URL], check=True)
        if digest(partial) != IMAGE_SHA256:
            raise SystemExit('Image SHA256 mismatch')
        partial.rename(base)
    if digest(base) != IMAGE_SHA256:
        raise SystemExit('Cached image SHA256 mismatch')
    report = {'test_source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'image': IMAGE, 'image_sha256': IMAGE_SHA256, 'package_sha256': digest(package),
              'package': package.name, 'memory_mib': 2048, 'cpus': 2, 'status': 'running'}
    (logs / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    with tempfile.TemporaryDirectory(prefix='decklock-vm-') as directory:
        work = Path(directory)
        key = work / 'id_ed25519'
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(key)], check=True)
        seed = work / 'seed'
        seed.mkdir()
        public = key.with_suffix('.pub').read_text().strip()
        (seed / 'meta-data').write_text('instance-id: decklock-isolated-test\nlocal-hostname: decklock-test-vm\n')
        (seed / 'user-data').write_text('#cloud-config\ndisable_root: false\nssh_pwauth: false\nusers:\n  - name: root\n    ssh_authorized_keys:\n      - ' + public + '\nwrite_files:\n  - path: /etc/decklock-test-vm\n    content: disposable-qemu-fixture\nruncmd:\n  - [systemctl, enable, --now, sshd]\n')
        subprocess.run(['genisoimage', '-quiet', '-output', str(work / 'seed.iso'), '-volid', 'cidata', '-joliet', '-rock', str(seed)], check=True)
        subprocess.run([shutil.which('qemu-img'), 'create', '-q', '-f', 'qcow2', '-F', 'qcow2', '-b', str(base), str(work / 'disk.qcow2'), '12G'], check=True)
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
                subprocess.run(['scp', *ssh_options, '-P', str(port), str(package), *map(str, sorted(p for p in (ROOT / 'scripts/vm').iterdir() if p.suffix in {'.py', '.sh'})), 'root@127.0.0.1:/root/decklock-fixture/'], check=True, timeout=60)
                print('Guest ready; installing dependencies and exercising the candidate.', flush=True)
                with (logs / 'guest.log').open('w') as guest_log:
                    result = subprocess.run([*ssh, 'bash /root/decklock-fixture/setup.sh'], stdout=guest_log, stderr=subprocess.STDOUT, timeout=1200)
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
    if report['status'] != 'passed':
        raise SystemExit('FAIL: VM evidence incomplete')
    print('PASS: real Sway/PAM package VM', flush=True)


if __name__ == '__main__':
    main()
