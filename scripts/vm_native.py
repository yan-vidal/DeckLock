"""Install a guest's harness packages on the host, before the guest boots.

Host and guest are both aarch64 on the hosted arm64 runners, so the package
installation that dominates an emulated run (about 70 of its 94 minutes) can run
natively in a chroot of the guest's disk instead of inside the TCG guest. Only that
step moves. The candidate package, the reinstall check, the PAM fixtures and every
suite still run in the booted guest, and scripts/vm/setup.sh is unchanged:
installing already-installed packages there is a no-op, which is also why the
in-guest install stays a valid fallback.

All mounting happens inside a private mount namespace, so nothing propagates to the
host (a bind of /dev under systemd's shared mounts would otherwise reach the host's
/dev/pts) and nothing is left mounted when the namespace ends. Every privileged
action is a logged command, and the runner is a parameter, so the ordering and
cleanup contract is tested without root.
"""
import os
import platform
import re
import shlex
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

GUEST_DISK_GIB = 12
MIN_FREE_GIB = 10
ROOT_LABEL = 'cloudimg-rootfs'
REQUIRED_TOOLS = ('qemu-img', 'losetup', 'lsblk', 'unshare', 'chroot',
                  'growpart', 'e2fsck', 'resize2fs')
ENV_IN_GUEST = '/tmp/decklock-target.env'


class NativeUnavailable(Exception):
    """A precondition does not hold; the caller falls back to the in-guest install."""


def read_manifest(path):
    return dict(re.findall(r"^([A-Z_]+)='([^']*)'", Path(path).read_text(), re.MULTILINE))


def provisioning_script(env_path=ENV_IN_GUEST, candidate=None):
    """The commands scripts/vm/setup.sh runs first, from the same manifest.

    With a candidate and a manifest that says how to purge it, the candidate's dependency
    closure is installed too: the candidate goes in with the manifest's own command and comes
    out again, dependencies staying. setup.sh then still installs the candidate itself in the
    booted guest, from a system that never had it, and apt's resolution is in provision.log.
    """
    script = (
        'set -euo pipefail\n'
        'export DEBIAN_FRONTEND=noninteractive LC_ALL=C\n'
        f'source {shlex.quote(env_path)}\n'
        '[[ -z $PRE_SYNC ]] || $PRE_SYNC\n'
        '$SYNC_AND_INSTALL $HARNESS_PACKAGES\n')
    if candidate:
        script += ('if [[ -n ${PURGE_CANDIDATE:-} ]]; then\n'
                   f'    $INSTALL_CANDIDATE {shlex.quote(candidate)}\n'
                   '    $PURGE_CANDIDATE\n'
                   'fi\n')
    return script


def chroot_script(partition, mount, manifest, policy, candidate=None, disable=()):
    """Runs under `unshare --mount --propagation private`: mount, prepare, install, restore.

    The EXIT trap is installed before anything is prepared and undoes only what was done,
    so the guest's own files come back whether the install succeeds or any step fails.
    The mounts need no undoing: they die with the namespace.
    """
    q = shlex.quote
    inside = f'/tmp/decklock-candidate{Path(str(candidate)).suffix}' if candidate else None
    copy_candidate = f'cp {q(str(candidate))} "$root{inside}"\n' if candidate else ''
    drop_candidate = f' "$root{inside}"' if candidate else ''
    disabled = ''.join(f'systemctl --root="$root" disable {q(unit)}\n' for unit in disable)
    return (
        'set -euo pipefail\n'
        f'root={q(str(mount))}\n'
        'cleanup() {\n'
        f'    rm -f "$root/usr/sbin/policy-rc.d" "$root{ENV_IN_GUEST}"{drop_candidate}\n'
        '    if [ "${moved:-0}" = 1 ]; then\n'
        '        rm -f "$root/etc/resolv.conf"\n'
        '        mv "$root/etc/resolv.conf.decklock-orig" "$root/etc/resolv.conf"\n'
        '    fi\n'
        '}\n'
        'trap cleanup EXIT\n'
        f'mount {q(str(partition))} "$root"\n'
        'mount --rbind /dev "$root/dev"\n'
        'mount -t proc proc "$root/proc"\n'
        f'install -m 0755 {q(str(policy))} "$root/usr/sbin/policy-rc.d"\n'
        f'cp {q(str(manifest))} "$root{ENV_IN_GUEST}"\n'
        + copy_candidate +
        'mv "$root/etc/resolv.conf" "$root/etc/resolv.conf.decklock-orig"\n'
        'moved=1\n'
        'cp /etc/resolv.conf "$root/etc/resolv.conf"\n'
        f'chroot "$root" /bin/bash -c {q(provisioning_script(candidate=inside))}\n'
        + disabled)


def guest_manifest(path):
    """The manifest the guest sees when its packages were installed on the host.

    setup.sh is not edited: it sources this file, and later assignments win, so its
    `apt-get update` and its install of already-installed harness packages become no-ops.
    The copy in the evidence directory shows exactly this.
    """
    return (Path(path).read_text().rstrip('\n') + '\n'
            '# Appended by test-vm.py: the harness packages were installed on the host (provision.log).\n'
            "PRE_SYNC=''\n"
            "SYNC_AND_INSTALL='true'\n")


def check_preconditions(*, guest_arch, host_arch, privileged, free_bytes, which):
    if guest_arch != host_arch:
        raise NativeUnavailable(
            f'the guest is {guest_arch} but the host is {host_arch}; a chroot needs the same architecture')
    missing = [tool for tool in REQUIRED_TOOLS if not which(tool)]
    if missing:
        raise NativeUnavailable('missing tools: ' + ', '.join(missing))
    if not privileged:
        raise NativeUnavailable('needs root or passwordless sudo')
    if free_bytes < MIN_FREE_GIB * 2**30:
        raise NativeUnavailable(f'{free_bytes / 2**30:.1f} GiB free, {MIN_FREE_GIB} GiB needed')


def host_facts(guest_arch, directory):
    try:
        privileged = os.geteuid() == 0 or subprocess.run(
            ['sudo', '-n', 'true'], capture_output=True, timeout=10).returncode == 0
    except (OSError, subprocess.SubprocessError):
        privileged = False
    return dict(guest_arch=guest_arch, host_arch=platform.machine(), privileged=privileged,
                free_bytes=shutil.disk_usage(directory).free, which=shutil.which)


def mounts_under(directory, mountinfo):
    """Mount points at or below `directory`, from /proc/self/mountinfo text."""
    prefix = str(directory).rstrip('/')
    points = []
    for line in mountinfo.splitlines():
        fields = line.split()
        if len(fields) > 4:
            point = fields[4].replace('\\040', ' ')
            if point == prefix or point.startswith(prefix + '/'):
                points.append(point)
    return points


class Host:
    """Runs commands, with sudo when not root, and logs each one with its duration."""

    def __init__(self, log, run=subprocess.run, geteuid=os.geteuid):
        self.log, self.run, self.root = log, run, geteuid() == 0

    def __call__(self, argv, *, timeout, check=True, sudo=True, capture=False):
        command = [*(['sudo', '-n'] if sudo and not self.root else []), *map(str, argv)]
        self.log.write('$ ' + shlex.join(command) + '\n')
        self.log.flush()
        started = time.monotonic()
        result = self.run(command, stdout=subprocess.PIPE if capture else self.log,
                          stderr=self.log if capture else subprocess.STDOUT,
                          text=True, timeout=timeout)
        if capture and result.stdout:
            self.log.write(result.stdout)
        self.log.write(f'# exit {result.returncode} after {time.monotonic() - started:.1f}s\n')
        self.log.flush()
        if check and result.returncode:
            raise subprocess.CalledProcessError(result.returncode, command)
        return result


def root_partition(host, loop, settle=time.sleep):
    for _ in range(20):
        listing = host(['lsblk', '-nr', '-o', 'PATH,LABEL', loop], timeout=30, capture=True).stdout
        for line in listing.splitlines():
            fields = line.split(' ', 1)
            if len(fields) == 2 and fields[1].strip() == ROOT_LABEL:
                return fields[0]
        settle(0.5)
    raise NativeUnavailable(f'no partition labelled {ROOT_LABEL} on {loop}')


def provision(base_image, manifest, work, log_path, *, package=None, disable=(), run=subprocess.run,
              geteuid=os.geteuid, size_gib=GUEST_DISK_GIB, settle=time.sleep,
              leftover=lambda mount: mounts_under(mount, Path('/proc/self/mountinfo').read_text())):
    """Copy the pinned image into `work`, install the harness packages into the copy, return its path.

    The mount point is a fresh directory outside `work` that is only ever removed with
    rmdir: were a mount ever left behind, a recursive delete of the work directory must
    not be able to reach a live bind of the host's /dev.
    """
    work = Path(work)
    raw = work / 'disk.raw'
    policy = work / 'policy-rc.d'
    policy.write_text('#!/bin/sh\nexit 101\n')
    mount = Path(tempfile.mkdtemp(prefix='decklock-root-'))
    with Path(log_path).open('w') as log:
        host = Host(log, run, geteuid)
        try:
            host(['qemu-img', 'convert', '-O', 'raw', base_image, raw], timeout=900, sudo=False)
            if raw.stat().st_size > size_gib * 2**30:
                raise NativeUnavailable(f'the base image is larger than the {size_gib} GiB guest disk')
            os.truncate(raw, size_gib * 2**30)
            loop = host(['losetup', '--find', '--show', '-P', raw], timeout=60, capture=True).stdout.strip()
            try:
                partition = root_partition(host, loop, settle)
                host(['growpart', loop, re.search(r'(\d+)$', partition).group(1)], timeout=120)
                if host(['e2fsck', '-fy', partition], timeout=300, check=False).returncode > 1:
                    raise subprocess.CalledProcessError(4, ['e2fsck', '-fy', partition])
                host(['resize2fs', partition], timeout=600)
                host(['unshare', '--mount', '--propagation', 'private', 'bash', '-c',
                      chroot_script(partition, mount, manifest, policy, candidate=package,
                                    disable=disable)], timeout=1800)
            finally:
                host(['sync'], timeout=120, check=False)
                host(['losetup', '-d', loop], timeout=60)
        finally:
            remaining = leftover(mount)
            if remaining:
                raise RuntimeError(f'mounts left under {mount}, refusing to continue: {remaining}')
            os.rmdir(mount)
    return raw


def discard(work):
    """Remove the raw copy; never anything recursive."""
    try:
        (Path(work) / 'disk.raw').unlink()
    except FileNotFoundError:
        pass


def describe(error):
    reason = f'{type(error).__name__}: {error}'
    if error.__context__ is not None:
        reason += f' (while handling {type(error.__context__).__name__}: {error.__context__})'
    return reason


def try_native(base_image, manifest, work, logs, guest_arch, *, run=subprocess.run, facts=host_facts,
               **options):
    """Return (raw_disk, None), or (None, reason) when the caller should boot the in-guest path."""
    try:
        check_preconditions(**facts(guest_arch, work))
        return provision(base_image, manifest, work, Path(logs) / 'provision.log', run=run, **options), None
    except (NativeUnavailable, subprocess.SubprocessError, OSError, RuntimeError) as error:
        discard(work)
        return None, describe(error)
