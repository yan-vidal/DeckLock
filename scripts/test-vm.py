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
import glob
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time

import vm_native

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
        # Both architectures are fetched from the dated serial their hash belongs to.
        # releases/26.04/release/ follows the newest serial, so it stopped matching the
        # x86_64 hash once Canonical published a new one; a serial directory does not move.
        'base': 'https://cloud-images.ubuntu.com/releases/resolute/release-20260918/',
        'suffix': '.deb',
        'aarch64': {
            'image': 'ubuntu-26.04-server-cloudimg-arm64.img',
            'sha256': '8dc812bc6356d0abf825d8029f25f1b71f02cb103e1d0cc5c17fbb2572322972',
            'base': 'https://cloud-images.ubuntu.com/releases/resolute/release-20260918/',
        },
    },
}
# UEFI firmware for the aarch64 virt machine, as Debian/Ubuntu, Arch and Fedora
# install it.
AARCH64_FIRMWARE = ['/usr/share/qemu-efi-aarch64/QEMU_EFI.fd', '/usr/share/AAVMF/AAVMF_CODE.fd',
                    '/usr/share/edk2/aarch64/QEMU_EFI.fd']
# qemu-img 2.x is still on some PATHs ahead of the system copy, for instance from
# a bundled Android SDK. Refuse it rather than creating an overlay with a tool
# that old and failing later in a way that looks like a guest problem.
MINIMUM_QEMU_IMG = (6, 0)
# The suites of scripts/vm/suites.sh, in the order a guest runs them. A KVM guest runs
# them all. Emulated guests share them, each suite in exactly one guest.
SUITES = ('package', 'session', 'compositors', 'greeter')


def selected_suites(text):
    names = text.split('+')
    if len(set(names)) != len(names) or any(name not in SUITES for name in names):
        raise argparse.ArgumentTypeError(f"distinct suites out of {', '.join(SUITES)}, joined by '+'")
    return [name for name in SUITES if name in names]


def completion_marks(suites, manifest):
    """The phase marks setup.sh prints once each of these suites has finished."""
    compositors = vm_native.read_manifest(manifest).get('EXTRA_COMPOSITORS', '').split()
    marks = {'package': ['candidate_reinstalled'], 'session': ['sway_pam_x11_complete'],
             'compositors': [f'{name}_complete' for name in compositors], 'greeter': ['greeter_complete']}
    return [mark for suite in suites for mark in marks[suite]]


def phase_marks(log):
    """(phase, guest clock seconds) for every mark in the guest's log."""
    if not log.exists():
        return []
    return [(fields[1], float(fields[2])) for line in log.read_text(errors='replace').splitlines()
            if (fields := line.split()) and len(fields) == 3 and fields[0] == 'DECKLOCK_VM_PHASE']


def cpu_model(arch, kvm):
    if kvm:
        return 'host'
    if arch == 'aarch64':
        # Before QEMU 10.0, `max` signs pointers with QARMA5, which cost half of an
        # emulated guest's time: 33m21s against 16m54s for the same 41 assertions (run
        # 38012670359, QEMU 8.2.2). The implementation-defined algorithm keeps the
        # instructions; it is no evidence of pointer-authentication strength.
        return 'max,pauth=on,pauth-impdef=on,pauth-qarma3=off'
    return 'max'


def candidate(pattern, suffix, target):
    """The single file --package names: a plain path, or a glob a fetch command filled."""
    matches = sorted(glob.glob(str(pattern)))
    if len(matches) != 1:
        raise SystemExit(f'--package must name exactly one file; {pattern} matches {len(matches)}')
    package = Path(matches[0]).resolve(strict=True)
    if not package.name.endswith(suffix):
        raise SystemExit(f'{target} expects a {suffix} candidate, got {package.name}')
    return package


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
    parser.add_argument('--package', type=Path,
                        help='Candidate package built by CI (required for execution). With '
                             '--package-fetch, a quoted glob that the command fills')
    parser.add_argument('--package-fetch', metavar='COMMAND',
                        help='Run COMMAND while the guest boots, then require it to have exited 0 and '
                             '--package to match one file. For CI, where the candidate is still building')
    parser.add_argument('--package-fetch-timeout', type=float, default=3600,
                        help='Seconds, from the start of COMMAND, the booted guest waits for it')
    parser.add_argument('--target', choices=sorted(DISTROS), default='arch')
    parser.add_argument('--arch', choices=['x86_64', 'aarch64'], default=platform.machine(),
                        help='Guest architecture; KVM needs it to match the host')
    parser.add_argument('--allow-emulation', action='store_true',
                        help='Without usable KVM, emulate the guest (TCG) with scaled waits. '
                             'For disposable CI runners only, never a shared machine')
    parser.add_argument('--provision-native', action='store_true',
                        help='Install the harness packages on the host before the guest boots. '
                             'Runs loop, growpart, resize2fs and chroot as root through sudo, so it is for '
                             'disposable CI runners only, never a shared machine. Needs the host and '
                             'guest architectures to match; without passwordless sudo, or on any failure, '
                             'it warns and installs inside the guest as usual')
    parser.add_argument('--logs', type=Path, default=ROOT / 'target/vm-logs')
    parser.add_argument('--suites', type=selected_suites, default=list(SUITES), metavar='A+B',
                        help=f"Suites this guest runs, out of {', '.join(SUITES)}; all by default. Guests "
                             'that share them must together run each one')
    parser.add_argument('--print-plan', action='store_true',
                        help='Print pinned image and CPU options without inspecting or starting a VM')
    args = parser.parse_args()
    distro = DISTROS[args.target]
    if args.arch != 'x86_64':
        if args.arch not in distro:
            raise SystemExit(f'No pinned {args.arch} image for {args.target}')
        distro = distro | distro[args.arch]
    manifest = ROOT / 'scripts/vm/distros' / f'{args.target}.env'
    if args.print_plan:
        print(json.dumps({'target': args.target, 'arch': args.arch,
                          'image': distro['image'], 'image_sha256': distro['sha256'],
                          'memory_mib': 2048, 'suites': args.suites,
                          'completion_marks': completion_marks(args.suites, manifest),
                          'cpu_models': {mode: cpu_model(args.arch, mode == 'kvm')
                                         for mode in ['kvm', 'tcg']},
                          'cpus': {'kvm': 2, 'tcg': 4}, 'slowdown': {'kvm': 1, 'tcg': 8}}, indent=2))
        return
    if args.package is None:
        parser.error('the following arguments are required: --package')
    # Only the fetch command may use the run's token: sudo, the emulator and ssh never see it.
    fetch_env = os.environ.copy()
    for name in ('GH_TOKEN', 'GITHUB_TOKEN'):
        os.environ.pop(name, None)
    started = time.monotonic()
    kvm = args.arch == platform.machine() and os.access('/dev/kvm', os.R_OK | os.W_OK)
    if not kvm and not args.allow_emulation:
        raise SystemExit('KVM required: refusing slow software emulation on the shared machine')
    # An emulated guest runs several times slower than one under KVM. Every wait
    # inside the guest and on this side scales by this estimate, never below 1.
    slowdown = 1 if kvm else 8
    qemu = f'qemu-system-{args.arch}'
    selected_cpu = cpu_model(args.arch, kvm)
    package = None if args.package_fetch else candidate(args.package, distro['suffix'], args.target)
    logs = args.logs.resolve()
    logs.mkdir(parents=True, exist_ok=True)
    for name in [qemu, 'genisoimage', 'curl', 'ssh', 'scp', 'ssh-keygen']:
        if not shutil.which(name):
            raise SystemExit(f'Missing dependency: {name}')
    image_tool = qemu_img()
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
              'target': args.target, 'arch': args.arch, 'image': distro['image'], 'image_sha256': distro['sha256'],
              'package_sha256': digest(package) if package else None,
              'package': package.name if package else None,
              'memory_mib': 2048, 'cpus': 2 if kvm else 4, 'accelerator': 'kvm' if kvm else 'tcg',
              'cpu_model': selected_cpu, 'suites': args.suites,
              'qemu_version': subprocess.check_output([qemu, '--version'], text=True, timeout=10).splitlines()[0],
              'host_phase_seconds': {},
              'slowdown': slowdown, 'provisioning': 'in-guest', 'status': 'running'}
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
        disk = f'file={work}/disk.qcow2,if=virtio,format=qcow2'
        guest_env = ROOT / 'scripts/vm/distros' / f'{args.target}.env'
        preparation_started = time.monotonic()
        if args.provision_native:
            manifest = ROOT / 'scripts/vm/distros' / f'{args.target}.env'
            raw, reason = vm_native.try_native(
                base, manifest, work, logs, args.arch,
                disable=vm_native.read_manifest(manifest).get('DISABLE_UNITS', '').split())
            if raw is not None:
                disk = f'file={raw},if=virtio,format=raw'
                guest_env = work / f'{args.target}.env'
                guest_env.write_text(vm_native.guest_manifest(manifest))
                report['provisioning'] = 'native'
            else:
                report['fallback_reason'] = reason
                print(f"::warning title=Native provisioning skipped::{reason.replace(chr(10), ' ')}", flush=True)
        if report['provisioning'] == 'in-guest':
            subprocess.run([image_tool, 'create', '-q', '-f', 'qcow2', '-F', 'qcow2', '-b', str(base), str(work / 'disk.qcow2'), '12G'], check=True)
        report['host_phase_seconds']['disk_and_harness'] = round(time.monotonic() - preparation_started, 3)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        accel = ['-enable-kvm'] if kvm else ['-accel', 'tcg,thread=multi']
        if args.arch == 'x86_64':
            machine = ['-cpu', selected_cpu]
            seed = ['-drive', f'file={work}/seed.iso,media=cdrom,readonly=on']
        else:
            # The virt machine has no IDE for a CD-ROM; cloud-init finds the seed
            # by its cidata label on any block device.
            firmware = next((path for path in AARCH64_FIRMWARE if Path(path).exists()), None)
            if firmware is None:
                raise SystemExit('Missing dependency: aarch64 UEFI firmware (qemu-efi-aarch64)')
            machine = ['-machine', 'virt,gic-version=' + ('host' if kvm else '3'),
                       '-cpu', selected_cpu, '-bios', firmware]
            seed = ['-drive', f'file={work}/seed.iso,if=virtio,format=raw,readonly=on']
        command = [qemu, '-name', 'decklock-isolated-test', *accel, *machine,
                   '-m', '2048', '-smp', str(report['cpus']), '-display', 'none', '-monitor', 'none', '-no-reboot',
                   '-drive', disk, *seed,
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
            boot_started = time.monotonic()
            vm = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT)
            thread = threading.Thread(target=guard, args=(vm,), daemon=True)
            thread.start()
            fetch = fetch_log = None
            try:
                if args.package_fetch:
                    # The candidate is still building: its artifact is awaited while this
                    # guest boots, and nothing is installed before the command succeeds.
                    fetch_log = (logs / 'candidate-fetch.log').open('w')
                    fetch_started = time.monotonic()
                    fetch = subprocess.Popen(['bash', '-c', args.package_fetch], env=fetch_env, stdout=fetch_log,
                                             stderr=subprocess.STDOUT, start_new_session=True)
                print(f"VM started: pid={vm.pid}, {report['accelerator'].upper()}, 2048 MiB, {report['cpus']} CPUs, "
                      f'waits x{slowdown}. Logs: {logs}', flush=True)
                deadline = time.monotonic() + 240 * slowdown
                while True:
                    if vm.poll() is not None:
                        raise RuntimeError('VM exited before SSH; see serial.log/qemu.log')
                    if fetch and fetch.poll():
                        raise RuntimeError(f'Candidate fetch failed ({fetch.returncode}); see candidate-fetch.log')
                    ready = subprocess.run([*ssh, 'test -f /etc/decklock-test-vm'], capture_output=True, timeout=8 * slowdown)
                    if ready.returncode == 0:
                        break
                    if time.monotonic() > deadline:
                        raise TimeoutError('VM boot/SSH deadline exceeded')
                    time.sleep(2)
                if fetch:
                    report['host_phase_seconds']['guest_ready'] = round(time.monotonic() - boot_started, 3)
                    guest_ready = time.monotonic()
                    try:
                        code = fetch.wait(timeout=max(0, fetch_started + args.package_fetch_timeout - guest_ready))
                    except subprocess.TimeoutExpired:
                        raise TimeoutError(f'No candidate after {args.package_fetch_timeout:g} s; see candidate-fetch.log') from None
                    if code:
                        raise RuntimeError(f'Candidate fetch failed ({code}); see candidate-fetch.log')
                    package = candidate(args.package, distro['suffix'], args.target)
                    report.update(package_sha256=digest(package), package=package.name)
                    report['host_phase_seconds']['candidate_wait'] = round(time.monotonic() - guest_ready, 3)
                subprocess.run([*ssh, 'mkdir -p /root/decklock-fixture /var/tmp/decklock-evidence'], check=True, timeout=15 * slowdown)
                fixture = sorted(p for p in (ROOT / 'scripts/vm').iterdir() if p.suffix in {'.py', '.sh'})
                fixture.append(guest_env)
                subprocess.run(['scp', *ssh_options, '-P', str(port), str(package), *map(str, fixture), 'root@127.0.0.1:/root/decklock-fixture/'], check=True, timeout=60 * slowdown)
                report['host_phase_seconds']['boot_ssh_and_copy'] = round(time.monotonic() - boot_started, 3)
                print('Guest ready; installing dependencies and exercising the candidate.', flush=True)
                guest_started = time.monotonic()
                with (logs / 'guest.log').open('w') as guest_log:
                    # needrestart, an apt hook on Ubuntu images, scans every process after each install:
                    # about 14 minutes apiece under emulation, to report nothing a disposable guest needs.
                    quiet = 'NEEDRESTART_SUSPEND=1 ' if slowdown > 1 else ''
                    chosen = shlex.quote(' '.join(args.suites))
                    result = subprocess.run([*ssh, f'{quiet}DECKLOCK_VM_SLOWDOWN={slowdown} DECKLOCK_VM_SUITES={chosen} bash /root/decklock-fixture/setup.sh {args.target}'],
                                            stdout=guest_log, stderr=subprocess.STDOUT, timeout=(600 if report['provisioning'] == 'native' else 1200) * slowdown)
                report['host_phase_seconds']['guest_execution'] = round(time.monotonic() - guest_started, 3)
                if result.returncode:
                    raise RuntimeError(f'Guest test failed ({result.returncode}); see guest.log')
                # A guest that shares the suites must have finished its own and no other:
                # a selection the guest read differently would otherwise pass having run less.
                finished = [phase for phase, _ in phase_marks(logs / 'guest.log')]
                owed = completion_marks(args.suites, manifest)
                if [mark for mark in finished if mark in completion_marks(SUITES, manifest)] != owed:
                    raise RuntimeError(f"Guest finished {finished}, not {owed} for {'+'.join(args.suites)}")
                report['status'] = 'passed'
            except BaseException as error:
                report['status'] = 'failed'
                report['error'] = str(error)
                raise
            finally:
                evidence_started = time.monotonic()
                if fetch and fetch.poll() is None:
                    os.killpg(fetch.pid, signal.SIGTERM)
                    try:
                        fetch.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(fetch.pid, signal.SIGKILL)
                        fetch.wait()
                if fetch_log:
                    fetch_log.close()
                    # Which job and artifact the candidate came from belongs in the job log.
                    print('--- candidate-fetch.log ---', (logs / 'candidate-fetch.log').read_text(errors='replace').rstrip(),
                          '--- end candidate-fetch.log ---', sep='\n', flush=True)
                if vm.poll() is None:
                    try:
                        transfer = subprocess.run(['scp', *ssh_options, '-P', str(port), '-r', 'root@127.0.0.1:/var/tmp/decklock-evidence', str(logs)], timeout=30 * slowdown, check=False)
                        # Not evidence the suites need: they time each install step, which apt does not put in the journal.
                        subprocess.run(['scp', *ssh_options, '-P', str(port), 'root@127.0.0.1:/var/log/dpkg.log', 'root@127.0.0.1:/var/log/apt/history.log', str(logs)], timeout=30 * slowdown, check=False)
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
                report['host_phase_seconds']['evidence_and_shutdown'] = round(time.monotonic() - evidence_started, 3)
                report['total_seconds'] = round(time.monotonic() - started, 3)
                report['guest_phase_timestamps'] = [
                    {'phase': phase, 'guest_epoch_seconds': seconds}
                    for phase, seconds in phase_marks(logs / 'guest.log')]
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
    print(f"PASS: real Sway/PAM package VM on {args.target} {args.arch}, suites {'+'.join(args.suites)}", flush=True)


if __name__ == '__main__':
    main()
