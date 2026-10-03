#!/usr/bin/env python3
"""Contract of the guest provisioning fixture, checked without a VM.

The real boundary is a guest whose cloud-init finished with a recoverable error,
which cannot be produced on demand: one CI run hit it and provisioning aborted
with exit 2 and no recorded reason. What can be pinned here is the decision that
made it abort, and the evidence that would have explained it, with cloud-init
replaced by a stub that exits the way the real one does.
"""
import ast
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import types
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent.parent
VM_RUNNER = ROOT / "scripts/test-vm.py"
HELPER = ROOT / "scripts/vm/cloud-init-ready.sh"
SETUP = ROOT / "scripts/vm/setup.sh"
GREETER = ROOT / "scripts/vm/greeter.py"
WORKFLOW = ROOT / ".github/workflows/checks.yml"
COMPOSITOR = ROOT / "scripts/vm/compositor.py"
EXERCISE = ROOT / "scripts/vm/exercise.py"


def run_with_stub(exit_code, output="status: done"):
    """Run the helper with a cloud-init that prints `output` and exits `exit_code`."""
    with tempfile.TemporaryDirectory(prefix="decklock-fixture-") as directory:
        directory = Path(directory)
        stub = directory / "cloud-init"
        stub.write_text(f'#!/bin/bash\necho "{output}"\nexit {exit_code}\n')
        stub.chmod(0o755)
        detail = directory / "status.txt"
        environment = dict(os.environ, PATH=f"{directory}:{os.environ['PATH']}")
        result = subprocess.run(["bash", str(HELPER), str(detail)], env=environment,
                                capture_output=True, text=True, timeout=30)
        return result, detail.read_text() if detail.is_file() else None


def load_native():
    sys.path.insert(0, str(ROOT / "scripts"))
    try:
        import vm_native
    except ImportError as error:
        return None, [f"scripts/vm_native.py cannot be imported: {error}"]
    return vm_native, []


def check_provisioning_script(native):
    failures = []
    with tempfile.TemporaryDirectory(prefix="decklock-native-") as directory:
        env = Path(directory) / "target.env"
        for pre, expected in (("", "install a b\n"), ("echo sync", "sync\ninstall a b\n")):
            env.write_text(f"PRE_SYNC='{pre}'\nSYNC_AND_INSTALL='echo install'\nHARNESS_PACKAGES='a b'\n")
            result = subprocess.run(["bash", "-c", native.provisioning_script(str(env))],
                                    capture_output=True, text=True, timeout=20)
            if result.stdout != expected or result.returncode:
                failures.append(f"provisioning script with PRE_SYNC={pre!r} printed {result.stdout!r}, "
                                f"exit {result.returncode}: {result.stderr}")
    return failures


def check_native_preconditions(native):
    failures = []
    good = dict(guest_arch="aarch64", host_arch="aarch64", privileged=True,
                free_bytes=20 * 2**30, which=lambda tool: "/usr/bin/" + tool)
    native.check_preconditions(**good)
    cases = {
        "architecture": dict(host_arch="x86_64"),
        "missing tools": dict(which=lambda tool: None),
        "root or passwordless sudo": dict(privileged=False),
        "GiB free": dict(free_bytes=2**30),
    }
    for expected, change in cases.items():
        try:
            native.check_preconditions(**(good | change))
            failures.append(f"native provisioning was allowed despite: {expected}")
        except native.NativeUnavailable as error:
            if expected not in str(error):
                failures.append(f"the reason for {expected} does not name it: {error}")
    return failures


def stubs(directory, failing=None):
    """Executables that log their arguments, so the chroot script runs without root."""
    bin_dir = Path(directory) / "bin"
    bin_dir.mkdir()
    log = Path(directory) / "calls.log"
    for name in ("mount", "install", "cp", "mv", "rm", "chroot"):
        stub = bin_dir / name
        code = 1 if name == failing else 0
        stub.write_text(f'#!/bin/bash\necho "{name} $*" >> "{log}"\nexit {code}\n')
        stub.chmod(0o755)
    return bin_dir, log


def run_chroot_script(native, failing):
    with tempfile.TemporaryDirectory(prefix="decklock-native-") as directory:
        bin_dir, log = stubs(directory, failing)
        script = native.chroot_script("/dev/loop7p1", Path(directory) / "root",
                                      "/x/ubuntu.env", Path(directory) / "policy-rc.d")
        env = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}")
        result = subprocess.run(["bash", "-c", script], env=env, capture_output=True,
                                text=True, timeout=20)
        calls = log.read_text().splitlines() if log.exists() else []
        return result, calls


def check_chroot_script(native):
    failures = []
    for failing in (None, "chroot"):
        result, calls = run_chroot_script(native, failing)
        heads = [call.split(" ", 1)[0] for call in calls]
        label = f"(chroot {'fails' if failing else 'succeeds'})"
        if bool(result.returncode) != bool(failing):
            failures.append(f"chroot script exit {result.returncode} {label}: {result.stderr}")
        if "chroot" not in heads:
            failures.append(f"the install never ran {label}")
            continue
        order = heads.index("chroot")
        if not any(call.startswith("mount ") and "proc" in call for call in calls[:order]):
            failures.append(f"proc was not mounted before the chroot {label}")
        if not any("--rbind /dev" in call for call in calls[:order]):
            failures.append(f"/dev was not bound before the chroot {label}")
        after = calls[order + 1:]
        if not any(call.startswith("rm -f") and "policy-rc.d" in call for call in after):
            failures.append(f"policy-rc.d was left in the guest {label}")
        if not any(call.startswith("mv ") and call.endswith("/etc/resolv.conf") and "decklock-orig" in call
                   for call in after):
            failures.append(f"the guest's resolv.conf was not restored {label}")
    # A step that fails before the install must still leave the guest as it found it,
    # and must not "restore" a resolv.conf it never moved.
    for failing in ("cp", "mv"):
        result, calls = run_chroot_script(native, failing)
        label = f"({failing} fails early)"
        if not result.returncode:
            failures.append(f"an early failure was reported as success {label}")
        if any(call.startswith("chroot") for call in calls):
            failures.append(f"the install ran after a failed preparation {label}")
        if not any(call.startswith("rm -f") and "policy-rc.d" in call for call in calls):
            failures.append(f"policy-rc.d was not cleaned up {label}")
        if sum(call.startswith("mv ") for call in calls) > 1:
            failures.append(f"a resolv.conf that was never moved was restored {label}")
    return failures


class FakeRun:
    """Records commands; `fail` names a command that exits non-zero."""

    def __init__(self, fail=None, raw_size=0):
        self.commands, self.fail, self.raw_size = [], fail, raw_size

    def __call__(self, command, stdout=None, stderr=None, text=None, timeout=None):
        self.commands.append(list(command))
        argv = command[2:] if command[:2] == ["sudo", "-n"] else command
        output = ""
        if argv[0] == "qemu-img":
            with open(argv[-1], "wb") as image:
                image.truncate(self.raw_size)
        if argv[0] == "losetup" and "--find" in argv:
            output = "/dev/loop7\n"
        if argv[0] == "lsblk":
            output = "/dev/loop7p15 UEFI\n/dev/loop7p1 cloudimg-rootfs\n"
        return types.SimpleNamespace(returncode=1 if argv[0] == self.fail else 0, stdout=output)


def check_native_orchestration(native):
    failures = []
    for fail in (None, "unshare", "growpart"):
        with tempfile.TemporaryDirectory(prefix="decklock-native-") as directory:
            work = Path(directory)
            (work / "base.img").write_bytes(b"")
            runner = FakeRun(fail)
            try:
                native.provision(work / "base.img", work / "ubuntu.env", work, work / "provision.log",
                                 run=runner, geteuid=lambda: 1000, settle=lambda _: None,
                                 leftover=lambda mount: [])
                outcome = "ok"
            except Exception as error:  # the contract is about what ran, not the type
                outcome = type(error).__name__
            argvs = [c[2:] if c[:2] == ["sudo", "-n"] else c for c in runner.commands]
            heads = [a[0] for a in argvs]
            label = f"(fail={fail})"
            if (fail is None) != (outcome == "ok"):
                failures.append(f"provision outcome {outcome} {label}")
            if not all(c[:2] == ["sudo", "-n"] for c in runner.commands if c[0] != "qemu-img"):
                failures.append(f"a privileged command ran without sudo -n {label}")
            if runner.commands and runner.commands[0][0] != "qemu-img":
                failures.append(f"the image conversion should not need sudo {label}")
            if argvs[-1][:2] != ["losetup", "-d"]:
                failures.append(f"the loop device was not detached last {label}: {argvs[-1]}")
            if "sync" not in heads:
                failures.append(f"nothing was synced before detaching {label}")
            order = [h for h in heads if h in ("losetup", "lsblk", "growpart", "e2fsck", "resize2fs", "unshare")]
            wanted = ["losetup", "lsblk", "growpart", "e2fsck", "resize2fs", "unshare"]
            if fail is None and order[:6] != wanted:
                failures.append(f"unexpected order of provisioning steps {label}: {order}")
            if fail == "growpart" and "unshare" in heads:
                failures.append(f"the install ran after the partition failed to grow {label}")
            if any(a[0] == "growpart" and a[2] != "1" for a in argvs):
                failures.append(f"growpart did not target partition 1 {label}")
    # A base image bigger than the guest disk would be cut, not grown.
    with tempfile.TemporaryDirectory(prefix="decklock-native-") as directory:
        work = Path(directory)
        (work / "base.img").write_bytes(b"")
        runner = FakeRun(raw_size=2 * 2**30)
        try:
            native.provision(work / "base.img", work / "ubuntu.env", work, work / "provision.log",
                             run=runner, geteuid=lambda: 1000, settle=lambda _: None,
                             size_gib=1, leftover=lambda mount: [])
            failures.append("an oversized base image was truncated instead of refused")
        except native.NativeUnavailable:
            pass
        if any(command[:3] == ["sudo", "-n", "losetup"] for command in runner.commands):
            failures.append("a loop device was attached for an image that was refused")
    return failures


def check_native_fallback(native):
    failures = []
    with tempfile.TemporaryDirectory(prefix="decklock-native-") as directory:
        work = Path(directory)
        (work / "base.img").write_bytes(b"")

        def facts(host_arch):
            return lambda arch, where: dict(guest_arch=arch, host_arch=host_arch, privileged=True,
                                            free_bytes=2**40, which=lambda tool: tool)
        offline = dict(geteuid=lambda: 1000, settle=lambda _: None, leftover=lambda mount: [])
        disk, reason = native.try_native(work / "base.img", work / "e.env", work, work, "aarch64",
                                         facts=facts("x86_64"))
        if disk is not None or "architecture" not in (reason or ""):
            failures.append(f"an architecture mismatch did not fall back with a reason: {disk}, {reason}")
        disk, reason = native.try_native(work / "base.img", work / "e.env", work, work, "aarch64",
                                         facts=facts("aarch64"), run=FakeRun("unshare"), **offline)
        if disk is not None or not reason or "CalledProcessError" not in reason:
            failures.append(f"a failed install did not fall back with a reason: {disk}, {reason}")
        if (work / "disk.raw").exists():
            failures.append("the half-provisioned raw disk was kept after falling back")
        disk, reason = native.try_native(work / "base.img", work / "e.env", work, work, "aarch64",
                                         facts=facts("aarch64"), run=FakeRun(), **offline)
        if disk != work / "disk.raw" or reason is not None:
            failures.append(f"a clean provision did not return the raw disk: {disk}, {reason}")
        leaked = native.try_native(work / "base.img", work / "e.env", work, work, "aarch64",
                                   facts=facts("aarch64"), run=FakeRun(),
                                   **(offline | dict(leftover=lambda mount: [str(mount)])))
        if leaked[0] is not None:
            failures.append("a mount left behind did not stop provisioning")
    return failures


def check_mounts_under(native):
    info = ("1 0 8:1 / / rw - ext4 /dev/a rw\n"
            "2 1 0:5 / /tmp/root rw - devtmpfs d rw\n"
            "3 2 0:6 / /tmp/root/proc rw - proc p rw\n"
            "4 1 0:7 / /tmp/rootbeer rw - tmpfs t rw\n")
    got = native.mounts_under("/tmp/root", info)
    return [] if got == ["/tmp/root", "/tmp/root/proc"] else [f"mounts_under returned {got}"]


def check_native_wiring():
    failures = []
    source = VM_RUNNER.read_text()
    for needle, why in (
            ("--provision-native", "test-vm.py has no --provision-native flag"),
            ("vm_native.try_native(", "test-vm.py does not call vm_native.try_native"),
            ("report['provisioning']", "the report does not record how the guest was provisioned"),
            ("fallback_reason", "a fallback does not record its reason"),
            ("::warning", "a fallback does not raise a visible warning")):
        if needle not in source:
            failures.append(why)
    setup = SETUP.read_text()
    for line in ("[[ -z $PRE_SYNC ]] || $PRE_SYNC", "$SYNC_AND_INSTALL $HARNESS_PACKAGES"):
        if line not in setup:
            failures.append(f"setup.sh no longer runs `{line}`, which vm_native mirrors")
    return failures


def job_block(text, name):
    match = re.search(rf"^  {re.escape(name)}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", text,
                      re.MULTILINE | re.DOTALL)
    return match.group(1) if match else ""


def check_native_workflow():
    failures = []
    text = WORKFLOW.read_text()
    if "--provision-native" not in job_block(text, "vm-ubuntu-aarch64"):
        failures.append("the aarch64 VM job does not use native provisioning")
    for name in ("vm", "vm-fedora", "vm-ubuntu"):
        if "--provision-native" in job_block(text, name):
            failures.append(f"{name} runs under KVM and must stay on its own path")
    needs = re.search(r"^  required:.*?needs: \[([^\]]*)\]", text, re.MULTILINE | re.DOTALL)
    waited = {item.strip() for item in needs.group(1).split(",")} if needs else set()
    for gate in ("check", "package", "package-fedora", "package-ubuntu", "package-fedora-aarch64",
                 "package-ubuntu-aarch64", "vm", "vm-fedora", "vm-ubuntu", "vm-ubuntu-aarch64"):
        if gate not in waited:
            failures.append(f"the required aggregate no longer waits for {gate}")
    return failures


def check_sway_readiness():
    """exercise.py must wait for Sway to finish starting before its first swaymsg.

    swaymsg stops waiting for a reply after 3 s, and Sway only accepts IPC once it logs
    "Running compositor on wayland display", long after its sockets exist. Racing that
    left about 0.6 s of margin under emulation and lost it once at 0.06 s.
    """
    source = EXERCISE.read_text()
    marker = source.find("Running compositor on wayland display")
    first_call = source.find("run(['swaymsg'")
    if first_call < 0:
        return ["exercise.py no longer calls swaymsg, so the readiness check is stale"]
    if marker < 0:
        return ["exercise.py never waits for Sway to report that it is running"]
    if marker > first_call:
        return ["exercise.py calls swaymsg before waiting for Sway to report that it is running"]
    return []


def main():
    failures = []

    # A clean run is usable, and its status is recorded either way.
    result, detail = run_with_stub(0)
    if result.returncode != 0:
        failures.append(f"a finished cloud-init was rejected: {result.returncode} {result.stderr}")
    if not detail or "exited 0" not in detail:
        failures.append(f"the status of a clean run was not recorded: {detail!r}")

    # The regression: "degraded done" is exit 2. The guest boots, installs and
    # runs the test, so provisioning must continue instead of failing the run.
    result, detail = run_with_stub(2)
    if result.returncode != 0:
        failures.append(
            f"a recoverable cloud-init error took the run down: {result.returncode} {result.stderr}")
    if not detail or "exited 2" not in detail:
        failures.append(f"the recoverable error was not recorded as evidence: {detail!r}")

    # A cloud-init that actually failed leaves a guest that proves nothing.
    result, _ = run_with_stub(1, output="status: error")
    if result.returncode == 0:
        failures.append("a failed cloud-init was accepted as a usable guest")

    # The decision has to be the one provisioning uses, not a script nobody calls.
    setup = SETUP.read_text()
    if "cloud-init-ready.sh" not in setup:
        failures.append("setup.sh does not use the fixture readiness check")
    if any(line.strip().startswith("cloud-init status") for line in setup.splitlines()):
        failures.append("setup.sh still waits on cloud-init directly, bypassing the check")
    if 'python3 "$fixture/greeter.py"' not in setup:
        failures.append("setup.sh does not exercise the packaged greetd login")
    try:
        compile(GREETER.read_text(), str(GREETER), 'exec')
    except SyntaxError as error:
        failures.append(f"greeter VM fixture does not parse: {error}")
    # The compositor list is read without running the guest-only script.
    known = set()
    try:
        tree = ast.parse(COMPOSITOR.read_text(), str(COMPOSITOR))
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign) and any(
                    isinstance(t, ast.Name) and t.id == 'COMPOSITORS' for t in node.targets):
                known = {key.value for key in getattr(node.value, 'keys', [])
                         if isinstance(key, ast.Constant)}
    except SyntaxError as error:
        failures.append(f"compositor VM fixture does not parse: {error}")
    if not known:
        failures.append("compositor.py names no compositor to exercise")
    if 'compositor.py "$compositor"' not in setup or 'for compositor in $EXTRA_COMPOSITORS' not in setup:
        failures.append("setup.sh does not exercise each manifest's further compositors")
    for target in ('arch', 'fedora', 'ubuntu'):
        manifest = (ROOT / f'scripts/vm/distros/{target}.env').read_text()
        if 'greetd cage' not in manifest:
            failures.append(f"{target} VM does not install greetd and Cage")
        fields = dict(re.findall(r"^([A-Z_]+)='([^']*)'", manifest, re.MULTILINE))
        extra = fields.get('EXTRA_COMPOSITORS', '').split()
        if not extra:
            failures.append(f"{target} VM exercises no compositor besides Sway")
        for name in extra:
            if name not in known:
                failures.append(f"{target} VM names {name}, which compositor.py cannot start")
            if name not in fields.get('HARNESS_PACKAGES', '').split():
                failures.append(f"{target} VM exercises {name} without installing it")

    # Each image is pinned by SHA256, so the URL it is fetched from must not move
    # underneath that pin. Canonical's `release/` directory follows the newest serial:
    # when it advanced, the pinned Ubuntu image stopped matching and every run of that
    # target failed with "Image SHA256 mismatch", whatever change was under test.
    # Like the compositor list above, the table is read without running the script.
    distros = {}
    try:
        for node in ast.parse(VM_RUNNER.read_text(), str(VM_RUNNER)).body:
            if isinstance(node, ast.Assign) and any(
                    isinstance(t, ast.Name) and t.id == 'DISTROS' for t in node.targets):
                distros = ast.literal_eval(node.value)
    except (SyntaxError, ValueError) as error:
        failures.append(f"test-vm.py does not expose its pinned images as a literal: {error}")
    if not distros:
        failures.append("test-vm.py names no pinned image to check")
    moving = {'release', 'current', 'latest', 'daily', 'pending'}
    for name, distro in distros.items():
        entries = {name: distro}
        entries.update({f'{name}/{key}': value for key, value in distro.items()
                        if isinstance(value, dict)})
        for label, entry in entries.items():
            path = {part for part in urlsplit(entry['base']).path.split('/') if part}
            if path & moving:
                failures.append(
                    f"{label} image is pinned by SHA256 but fetched from {entry['base']}, "
                    f"a directory that moves ({', '.join(sorted(path & moving))}); "
                    "use the dated directory the hash belongs to")

    native, problems = load_native()
    failures.extend(problems)
    if native is not None:
        for check in (check_provisioning_script, check_native_preconditions, check_chroot_script,
                      check_native_orchestration, check_native_fallback, check_mounts_under):
            failures.extend(check(native))

    failures.extend(check_native_wiring())
    failures.extend(check_sway_readiness())
    failures.extend(check_native_workflow())

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS: cloud-init readiness and native provisioning contracts")


if __name__ == "__main__":
    main()
