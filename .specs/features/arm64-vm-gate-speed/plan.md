# Native provisioning for the aarch64 VM gate: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut the required `vm-ubuntu-aarch64` gate from about 94 minutes to about 30 by installing the guest's harness packages on the host before the guest boots, without removing or weakening any assertion.

**Architecture:** A new host-side module, `scripts/vm_native.py`, copies the pinned cloud image to a raw disk, grows it, and installs `HARNESS_PACKAGES` in a chroot at native speed (host and guest are both aarch64). Everything privileged runs through a logged, injectable runner, and all mounting happens in a private mount namespace. `scripts/test-vm.py` gains `--provision-native`; on any failure it warns loudly and takes the unchanged in-guest path. `scripts/vm/setup.sh` and every assertion are untouched.

**Tech Stack:** Python 3.14 standard library, bash, GitHub Actions on `ubuntu-24.04-arm`, QEMU (TCG), util-linux (`unshare`, `losetup`, `lsblk`), cloud-guest-utils (`growpart`), e2fsprogs.

**Spec:** `.specs/features/arm64-vm-gate-speed/spec.md`. Read it first; `baseline-pass.txt` beside it lists the 41 assertions the faster gate must still print.

**Working tree:** run everything from `/home/yan/projetos/DeckLock/.worktrees/controller-reconnect`, on branch `ci/arm64-vm-native-provision`. Never touch the primary checkout at `/home/yan/projetos/DeckLock`; it holds the user's uncommitted work.

## Global Constraints

Copied from the spec and from `AGENTS.md`; every task includes them.

- No assertion is removed, skipped, weakened or reordered, and no required check is relaxed.
- The guest keeps installing the latest Ubuntu archive packages on every run. No cache, no frozen image, no published artifact, no self-hosted runner.
- `scripts/vm/setup.sh` is unchanged. The candidate install, the reinstall check, the PAM fixtures and every suite run in the booted guest.
- The x86_64 jobs (`vm`, `vm-fedora`, `vm-ubuntu`) stay on their KVM path and do not get `--provision-native`.
- The base image SHA256 is verified before use, as today. Every wait is bounded.
- If native provisioning cannot run or fails, the job warns visibly, records the reason in `result.json`, and continues on the in-guest path. It never passes silently on a different footing.
- Add a failing test before the code that makes it pass. Run `scripts/check` for code changes and `scripts/check --all` before the PR is marked ready. A timeout or missing dependency is a failure, never a skip.
- Commits and PR text carry no Claude attribution: no `Co-Authored-By` trailer and no "Generated with" line (the user's standing instruction). No force-push, no direct push to `main`, no bypass of required checks.
- Nothing here can be proven locally beyond the contract tests: the loop attach and chroot only run on the hosted arm64 runner, so each integration iteration costs one CI round trip.

## Review Focus

Failure modes the spec implies but a happy-path run would not show. Each has a test in the task named in brackets.

1. The hosted runner refuses `unshare`, `losetup` or `chroot`, or `sudo` would prompt: the gate must fall back with a reason and never hang. [Task 2: preconditions, `sudo -n`; Task 5: the probe]
2. A mount propagating back to the host's `/dev/pts`: everything mounts inside `unshare --mount --propagation private`. [Task 2: chroot-script test never sets propagation itself]
3. The guest left altered after a failure (`policy-rc.d`, the copied manifest, the swapped `resolv.conf`): the cleanup trap exists before any preparation and restores only what was done. [Task 2: success, `chroot` failing, `cp` failing early, `mv` failing early]
4. A base image larger than the 12 GiB guest disk being truncated instead of grown, or a half-built raw disk kept for the fallback. [Task 2: oversize guard; fallback discards the raw disk]
5. A recursive delete of the work directory reaching a mount that failed to unmount: the mount point is a separate directory removed only with `rmdir`, and a leftover mount stops the run. [Task 2: leftover-mount test]

## Execution notes (what the first CI probe changed)

The probe (PR #56, run 37092747097) proved the hosted arm64 runner allows the mechanism and
that the harness packages install natively in 36 s, and it found two things the plan did not
anticipate. Both were ruled on during execution and are now in the spec (R1, R2, baseline):

- The candidate's own 98 dependency packages still cost about 40 minutes in the guest, so
  the host step also installs the candidate and purges it again (`PURGE_CANDIDATE` in the
  manifest; `vm_native.provisioning_script(env_path, candidate)` and
  `chroot_script(..., candidate=)`). Commit `ci: install the candidate's dependencies on the
  host too`.
- `exercise.py` raced Sway's start-up against `swaymsg`'s 3 s timeout (margin about 0.6 s)
  and now waits for Sway's report first, with a contract test. Commit `ci: wait for Sway to
  be running before the first swaymsg`.

Because `chroot_script` and `provision` changed, the Task 6 and Task 7 diffs below were
written against the earlier signatures. They are re-derived against the code as it stands
when Task 6 is executed; their intent, tests and keep rules are unchanged.

## How to apply the diffs in this plan

Each `diff` block was generated by applying the previous tasks exactly as written and then running the contract test red and green. Apply one with `patch -p1 < block.diff` from the repository root (small line offsets are fine) or by hand; if it does not apply cleanly, an earlier task was not applied as written, and that is the thing to fix.

## File Structure

- Create `scripts/vm_native.py`: the whole native path (manifest parsing, the chroot script, preconditions, the logged host runner, `provision`, `try_native`). Deliberately outside `scripts/vm/`, because every file there is copied into the guest.
- Modify `scripts/test-vm-fixture.py`: the contract tests (it is the existing no-root `vm-fixture` gate, 60 s budget).
- Modify `scripts/test-vm.py`: the flag, the report fields, the disk selection.
- Modify `.github/workflows/checks.yml`: the aarch64 job only.
- Modify docs, `STATE.md` and the spec in the last task.

---

### Task 1: Align the spec with the safer design

The approved spec names `scripts/vm/provision_native.py` and unmounting by hand. Prototyping showed two problems: files in `scripts/vm/` are copied into the guest, and a bind of `/dev` plus `devpts` can propagate to the host under systemd's shared mounts. The design below fixes both; the spec must say so before code follows it.

**Files:**
- Modify: `.specs/features/arm64-vm-gate-speed/spec.md`

- [ ] **Step 1: Replace R5**

Replace this text:

```
- **R5 Containment.** The host only loop-mounts and chroots the harness's own work copy of
  the pinned image, under the work directory, never the base cache image or any host path.
  Mounts and loop devices are released on every exit path, including failure and timeout.
  No host home, desktop socket, credential or device is exposed. The base image SHA256 is
  verified before use, as today.
```

with:

```
- **R5 Containment.** The host only loop-attaches the harness's own work copy of the pinned
  image, never the base cache image or any host path. All mounting happens inside a private
  mount namespace (`unshare --mount --propagation private`), so nothing propagates to the
  host and nothing survives the namespace. The mount point is a fresh directory outside the
  work directory that is only ever removed with `rmdir`, so a recursive delete of the work
  directory cannot reach a mounted filesystem. The loop device is detached on every exit
  path, including failure and timeout. No host home, desktop socket, credential or device
  is exposed. The base image SHA256 is verified before use, as today.
```

- [ ] **Step 2: Replace R8**

Replace:

```
- **R8 Tests.** Command construction, `.env` parsing, the fallback decision, mount cleanup
  ordering (with injected runners) and the `result.json` fields are covered by the existing
  `vm-fixture` gate without root. The loop mount and chroot themselves are covered by the
  CI job, which is the integration test.
```

with:

```
- **R8 Tests.** Command construction, `.env` parsing, the fallback decision, the order of
  the host steps and the `result.json` fields are covered by the existing `vm-fixture` gate
  without root, with an injected runner. The chroot script's cleanup contract is covered by
  running the real script against logging stand-ins for `mount`, `chroot` and the file
  tools, including when a step fails early. The loop attach and the real chroot are covered
  by the CI job, which is the integration test.
```

- [ ] **Step 3: Replace R9**

A job timeout cannot be shorter than the sum of the script's own bounds without turning a stuck guest into a cancelled job with no log, which is why it is 300 today. Replace:

```
- **R9 Timeout and docs.** `timeout-minutes` drops from 300 to the measured duration plus a
  margin, with the figure and its source written next to it. `docs/vm-testing.md`,
  `docs/testing.md`, `docs/ROADMAP.md` and `.specs/project/STATE.md` describe the new flow
  and its limits.
```

with:

```
- **R9 Timeout and docs.** On the native path the guest phase is bounded at 600 s times the
  slowdown instead of 1200, and `timeout-minutes` drops from 300 to just above the sum of
  `test-vm.py`'s own bounds, so a stuck guest still ends in its own log. The comment beside
  it states the sum and the measured duration with its run. `docs/vm-testing.md`,
  `docs/testing.md`, `docs/ROADMAP.md` and `.specs/project/STATE.md` describe the new flow
  and its limits. The timeout is only the ceiling for a hang; the speed-up is the typical run.
```

- [ ] **Step 4: Replace the Design list**

Replace the line `Host side (new module \`scripts/vm/provision_native.py\`, called from \`scripts/test-vm.py\`):` with:

```
Host side (new module `scripts/vm_native.py`, called from `scripts/test-vm.py`; it lives
outside `scripts/vm/` because every file there is copied into the guest):
```

Replace item 1's tool list `` `losetup`, `mount`,\n   `chroot`, `sfdisk` or `growpart`, `e2fsck`, `resize2fs`, free space (R7). `` (two lines in the file) with:

```
`losetup`, `lsblk`,
   `unshare`, `chroot`, `growpart`, `e2fsck`, `resize2fs`, free space (R7).
```

Replace items 3 and 6 with:

```
3. `losetup --find --show -P` the copy, then run one script under
   `unshare --mount --propagation private`: mount the root, bind `/dev`, mount `/proc`,
   install a `policy-rc.d` that refuses service starts, swap in the host's `resolv.conf`,
   and run the install in a chroot.
```

```
6. The guest's own files are restored by an `EXIT` trap installed before anything is
   prepared, so it also runs when a step fails early. The mounts end with the namespace.
   `sync`, then detach the loop device, always (`try/finally`).
```

and add to item 2, after "(`cloudimg-rootfs`).": ` A base image larger than the guest disk is refused rather than truncated.`

- [ ] **Step 5: Update the status line and commit**

Change `Status: design approved by the user on 2026-10-02; written spec awaiting review.` to `Status: spec approved on 2026-10-02; amended on 2026-10-03 for the private mount namespace (R5, R8, Design); plan in plan.md.`

```bash
git add .specs/features/arm64-vm-gate-speed/spec.md
git diff --cached --check
git commit -m "docs: align the VM gate spec with a private mount namespace"
```

---

### Task 2: The native provisioning module and its contract tests

**Files:**
- Create: `scripts/vm_native.py`
- Modify: `scripts/test-vm-fixture.py`

**Interfaces:**
- Produces (used by Task 3): `vm_native.try_native(base_image, manifest, work, logs, guest_arch, **kw) -> (raw_path | None, reason | None)`; `NativeUnavailable`; `provision(...)`, `check_preconditions(...)`, `chroot_script(partition, mount, manifest, policy)`, `provisioning_script(env_path)`, `mounts_under(directory, mountinfo)`, `read_manifest(path)`.

- [ ] **Step 1: Write the failing tests**

In `scripts/test-vm-fixture.py`:

1. After `import tempfile` add `import types`.
2. Immediately above `def main():` insert exactly this block (and keep two blank lines before `def main():`):

```python
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
```

3. In `main()`, replace `    if failures:\n        for failure in failures:` with:

```python
    native, problems = load_native()
    failures.extend(problems)
    if native is not None:
        for check in (check_provisioning_script, check_native_preconditions, check_chroot_script,
                      check_native_orchestration, check_native_fallback, check_mounts_under):
            failures.extend(check(native))

    if failures:
        for failure in failures:
```

4. Change the final `print("PASS: cloud-init readiness contract and its recorded evidence")` to `print("PASS: cloud-init readiness and native provisioning contracts")`.

- [ ] **Step 2: Run the tests and see them fail**

Run: `python3 scripts/test-vm-fixture.py`
Expected: exit 1 with `FAIL: scripts/vm_native.py cannot be imported: No module named 'vm_native'`

- [ ] **Step 3: Write the module**

Create `scripts/vm_native.py`:

```python
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


def provisioning_script(env_path=ENV_IN_GUEST):
    """The two commands scripts/vm/setup.sh runs first, from the same manifest."""
    return (
        'set -euo pipefail\n'
        'export DEBIAN_FRONTEND=noninteractive LC_ALL=C\n'
        f'source {shlex.quote(env_path)}\n'
        '[[ -z $PRE_SYNC ]] || $PRE_SYNC\n'
        '$SYNC_AND_INSTALL $HARNESS_PACKAGES\n')


def chroot_script(partition, mount, manifest, policy):
    """Runs under `unshare --mount --propagation private`: mount, prepare, install, restore.

    The EXIT trap is installed before anything is prepared and undoes only what was done,
    so the guest's own files come back whether the install succeeds or any step fails.
    The mounts need no undoing: they die with the namespace.
    """
    q = shlex.quote
    return (
        'set -euo pipefail\n'
        f'root={q(str(mount))}\n'
        'cleanup() {\n'
        f'    rm -f "$root/usr/sbin/policy-rc.d" "$root{ENV_IN_GUEST}"\n'
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
        'mv "$root/etc/resolv.conf" "$root/etc/resolv.conf.decklock-orig"\n'
        'moved=1\n'
        'cp /etc/resolv.conf "$root/etc/resolv.conf"\n'
        f'chroot "$root" /bin/bash -c {q(provisioning_script())}\n')


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


def provision(base_image, manifest, work, log_path, *, run=subprocess.run, geteuid=os.geteuid,
              size_gib=GUEST_DISK_GIB, settle=time.sleep,
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
                      chroot_script(partition, mount, manifest, policy)], timeout=1800)
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
```

- [ ] **Step 4: Run the tests and see them pass**

Run: `python3 scripts/test-vm-fixture.py`
Expected: `PASS: cloud-init readiness and native provisioning contracts`

Then: `scripts/check` (the `vm-fixture` step runs these tests with a 60 s budget; `whitespace` must stay clean).
Expected: all steps `PASS`.

- [ ] **Step 5: Commit**

```bash
git add scripts/vm_native.py scripts/test-vm-fixture.py
git commit -m "ci: add a host-side provisioning step for the aarch64 VM guest

Installs the harness packages into a copy of the pinned image in a chroot before
the guest boots, so they do not have to be installed under emulation. Nothing
calls it yet. The contract tests cover the host step order, the chroot script's
cleanup (including a step failing early), the preconditions, the fallback and
the refusal of an oversized image."
```

- [ ] **Step 6: Prove the tests can fail**

Temporarily break the contract and confirm the tests notice, then undo: in `scripts/vm_native.py` delete the line `        'trap cleanup EXIT\n'` and run `python3 scripts/test-vm-fixture.py`. Expected: `FAIL` lines naming `policy-rc.d` and `resolv.conf`. Restore with `git restore scripts/vm_native.py` and rerun to see `PASS`.

---

### Task 3: Wire it into `scripts/test-vm.py`

**Files:**
- Modify: `scripts/test-vm.py`, `scripts/test-vm-fixture.py`

**Interfaces:**
- Consumes: `vm_native.try_native` from Task 2.
- Produces: the `--provision-native` flag; `result.json` fields `provisioning` (`native` or `in-guest`) and `fallback_reason`; `provision.log` in the logs directory.

- [ ] **Step 1: Write the failing wiring test**

In `scripts/test-vm-fixture.py`, insert above `def main():`:

```python
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
```

and in `main()`, immediately before the final `if failures:`, add `    failures.extend(check_native_wiring())` followed by a blank line.

- [ ] **Step 2: Run it and see it fail**

Run: `python3 scripts/test-vm-fixture.py`
Expected: exit 1 with five `FAIL` lines: `test-vm.py has no --provision-native flag`, `does not call vm_native.try_native`, `the report does not record how the guest was provisioned`, `a fallback does not record its reason`, `a fallback does not raise a visible warning`.

- [ ] **Step 3: Edit `scripts/test-vm.py`**

Apply this diff (context lines are from the current file):

```diff
--- a/scripts/test-vm.py
+++ b/scripts/test-vm.py
@@ -25,6 +25,8 @@
 import threading
 import time

+import vm_native
+
 ROOT = Path(__file__).resolve().parent.parent
 DISTROS = {
     'arch': {
@@ -110,6 +112,10 @@
     parser.add_argument('--allow-emulation', action='store_true',
                         help='Without usable KVM, emulate the guest (TCG) with scaled waits. '
                              'For disposable CI runners only, never a shared machine')
+    parser.add_argument('--provision-native', action='store_true',
+                        help='Install the harness packages on the host before the guest boots. '
+                             'Needs the host and guest architectures to match and root; '
+                             'otherwise warns and installs inside the guest as usual')
     parser.add_argument('--logs', type=Path, default=ROOT / 'target/vm-logs')
     args = parser.parse_args()
     distro = DISTROS[args.target]
@@ -152,7 +158,7 @@
               'target': args.target, 'arch': args.arch, 'image': distro['image'], 'image_sha256': distro['sha256'],
               'package_sha256': digest(package), 'package': package.name,
               'memory_mib': 2048, 'cpus': 2 if kvm else 4, 'accelerator': 'kvm' if kvm else 'tcg',
-              'slowdown': slowdown, 'status': 'running'}
+              'slowdown': slowdown, 'provisioning': 'in-guest', 'status': 'running'}
     (logs / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
     # The copy-on-write overlay grows with every guest write, and a package
     # install writes hundreds of MiB. /tmp is tmpfs by default on Arch and Fedora,
@@ -168,7 +174,18 @@
         (seed / 'meta-data').write_text('instance-id: decklock-isolated-test\nlocal-hostname: decklock-test-vm\n')
         (seed / 'user-data').write_text('#cloud-config\ndisable_root: false\nssh_pwauth: false\nusers:\n  - name: root\n    ssh_authorized_keys:\n      - ' + public + '\nwrite_files:\n  - path: /etc/decklock-test-vm\n    content: disposable-qemu-fixture\nruncmd:\n  - [systemctl, enable, --now, sshd]\n')
         subprocess.run(['genisoimage', '-quiet', '-output', str(work / 'seed.iso'), '-volid', 'cidata', '-joliet', '-rock', str(seed)], check=True)
-        subprocess.run([image_tool, 'create', '-q', '-f', 'qcow2', '-F', 'qcow2', '-b', str(base), str(work / 'disk.qcow2'), '12G'], check=True)
+        disk = f'file={work}/disk.qcow2,if=virtio,format=qcow2'
+        if args.provision_native:
+            raw, reason = vm_native.try_native(
+                base, ROOT / 'scripts/vm/distros' / f'{args.target}.env', work, logs, args.arch)
+            if raw is not None:
+                disk = f'file={raw},if=virtio,format=raw'
+                report['provisioning'] = 'native'
+            else:
+                report['fallback_reason'] = reason
+                print(f"::warning title=Native provisioning skipped::{reason.replace(chr(10), ' ')}", flush=True)
+        if report['provisioning'] == 'in-guest':
+            subprocess.run([image_tool, 'create', '-q', '-f', 'qcow2', '-F', 'qcow2', '-b', str(base), str(work / 'disk.qcow2'), '12G'], check=True)
         with socket.socket() as sock:
             sock.bind(('127.0.0.1', 0))
             port = sock.getsockname()[1]
@@ -187,7 +204,7 @@
             seed = ['-drive', f'file={work}/seed.iso,if=virtio,format=raw,readonly=on']
         command = [qemu, '-name', 'decklock-isolated-test', *accel, *machine,
                    '-m', '2048', '-smp', str(report['cpus']), '-display', 'none', '-monitor', 'none', '-no-reboot',
-                   '-drive', f'file={work}/disk.qcow2,if=virtio,format=qcow2', *seed,
+                   '-drive', disk, *seed,
                    '-netdev', f'user,id=net0,hostfwd=tcp:127.0.0.1:{port}-:22',
                    '-device', 'virtio-net-pci,netdev=net0', '-device', 'virtio-rng-pci',
                    '-serial', f'file:{logs}/serial.log']
```

- [ ] **Step 4: Run the tests, compile, check the flag**

Run: `python3 scripts/test-vm-fixture.py`
Expected: `PASS: cloud-init readiness and native provisioning contracts`

Run: `python3 -m py_compile scripts/test-vm.py && python3 scripts/test-vm.py --help | grep -A3 -- '--provision-native'`
Expected: the flag and its help text.

Run: `scripts/check`
Expected: all `PASS`.

- [ ] **Step 5: Commit**

```bash
git add scripts/test-vm.py scripts/test-vm-fixture.py
git commit -m "ci: let the VM runner provision the guest natively, with a loud fallback

--provision-native tries the host-side install and boots the raw disk; on any
failure it records the reason in result.json, prints a ::warning:: annotation and
uses the unchanged qcow2 overlay with the in-guest install. result.json also
records how the guest was provisioned."
```

---

### Task 4: Turn it on for the aarch64 job only

**Files:**
- Modify: `.github/workflows/checks.yml`, `scripts/test-vm-fixture.py`

- [ ] **Step 1: Write the failing workflow test**

Insert above `def main():` in `scripts/test-vm-fixture.py`:

```python
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
```

Add `WORKFLOW = ROOT / ".github/workflows/checks.yml"` below the existing `GREETER = ...` constant, and in `main()` add `    failures.extend(check_native_workflow())` right after the `check_native_wiring()` line.

- [ ] **Step 2: Run it and see it fail**

Run: `python3 scripts/test-vm-fixture.py`
Expected: exit 1 with exactly `FAIL: the aarch64 VM job does not use native provisioning`.

- [ ] **Step 3: Edit the aarch64 job**

```diff
--- a/.github/workflows/checks.yml
+++ b/.github/workflows/checks.yml
@@ -220,7 +220,7 @@
       - name: VM dependencies
         run: |
           sudo apt-get update
-          sudo apt-get install -y qemu-system-arm qemu-efi-aarch64 qemu-utils genisoimage openssh-client
+          sudo apt-get install -y qemu-system-arm qemu-efi-aarch64 qemu-utils genisoimage openssh-client cloud-guest-utils
       - uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4
         with:
           name: ubuntu-package-aarch64
@@ -228,7 +228,7 @@
       - name: Verify candidate and run disposable VM
         run: |
           (cd dist && sha256sum --check SHA256SUMS)
-          python3 scripts/test-vm.py --target ubuntu --arch aarch64 --allow-emulation --package dist/*.deb
+          python3 scripts/test-vm.py --target ubuntu --arch aarch64 --allow-emulation --provision-native --package dist/*.deb
       - name: VM evidence
         if: always()
         uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4
```

`cloud-guest-utils` provides `growpart`; `util-linux`, `e2fsprogs` and `sudo` are already on the runner. Leave `timeout-minutes: 300` and the other jobs alone for now; Task 7 sets the measured value.

- [ ] **Step 4: Run the tests and the core gate**

Run: `python3 scripts/test-vm-fixture.py` then `scripts/check`
Expected: `PASS`, all steps `PASS`. The test also fails if the required aggregate stops waiting for any of the ten gates or if a KVM job gains the flag, which is how the "no required check relaxed" constraint is enforced.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/checks.yml scripts/test-vm-fixture.py
git commit -m "ci: provision the aarch64 VM natively before boot

Only the emulated Ubuntu arm64 job passes --provision-native, and installs
cloud-guest-utils for growpart. The x86_64 jobs, the required aggregate and the
job's timeout are unchanged. A contract test pins all three."
```

---

### Task 5: The probe: first CI run on the hosted arm64 runner

This is where the unknowns resolve. Do not improvise around a runner that forbids the mechanism.

**Files:** none until a failure needs a fix.

- [ ] **Step 1: Run the full local gate on the exact commit**

Run: `scripts/check --all`
Expected: `PASS: 15 gates`. The local machine can be loaded: the isolated GTK gate gives xfwm4 5 s to start, and a loaded host has failed that step with `Private xfwm4 did not publish its WM property` before. That is infrastructure: rerun when the load drops, and do not touch the harness. Record the commit with `git rev-parse HEAD`.

- [ ] **Step 2: Push and open a draft PR**

```bash
git push -u origin ci/arm64-vm-native-provision
gh pr create --draft --base main \
  --title "ci: provision the aarch64 VM natively before boot" \
  --body "Draft. Installs the harness packages on the host before the emulated aarch64 guest boots (spec: .specs/features/arm64-vm-gate-speed/spec.md). Evidence pending: the first CI run is a probe of whether the hosted arm64 runner allows the loop attach and chroot. The other jobs, assertions and required checks are unchanged."
```

- [ ] **Step 3: Watch the aarch64 job**

Find the run with `gh run list --branch ci/arm64-vm-native-provision --limit 3`, then wait with a Monitor that polls `gh run view <run-id> --json status,conclusion` every 60 s, and capture the job time:

```bash
gh run view <run-id> --json jobs --jq '.jobs[] | select(.name=="Real Wayland and PAM (Ubuntu aarch64)") | .steps[] | select(.name=="Verify candidate and run disposable VM") | "\(.startedAt) \(.completedAt)"'
```

- [ ] **Step 4: Read the evidence**

```bash
SCRATCH=$(mktemp -d)
gh run download <run-id> -n vm-evidence-ubuntu-aarch64 -D "$SCRATCH/evidence"
cat "$SCRATCH/evidence/result.json"
head -40 "$SCRATCH/evidence/provision.log"
```

Decide by `result.json`:

| `provisioning` | Next |
|---|---|
| `native`, `status: passed` | Go to Step 5. |
| `in-guest` with `fallback_reason` | The job passed the slow way. Read `provision.log` and the reason. A logic bug in `vm_native.py`: add a failing contract test for it first, fix, push, and repeat Step 3. The runner refusing `unshare`, `losetup`, `mount` or `chroot` (`Operation not permitted`, `Permission denied`, no `/dev/loop*`): **stop**, report the exact log lines to the user, and do not try another mechanism. |
| `native`, `status: failed` | The guest ran on the provisioned disk and a suite failed. Read `guest.log`, `serial.log` and `journal.log`; first rule out a chroot side effect (service state, `resolv.conf`, a package whose maintainer script needs a live systemd) before blaming the suite. |

- [ ] **Step 5: Check coverage was not reduced**

```bash
diff <(grep '^PASS' "$SCRATCH/evidence/guest.log" | sort) <(sort .specs/features/arm64-vm-gate-speed/baseline-pass.txt) && echo "same 41 assertions"
```

Expected: `same 41 assertions`. Any difference must be explained line by line (for example an assertion that legitimately changed on `main` since the baseline); a missing line is a reduction of coverage and blocks the change.

- [ ] **Step 6: Record the measurements**

Write down, from the run: step duration, the per-command durations in `provision.log` (each command is logged as `# exit N after Ns`), the boot time from the guest journal, and the baseline beside them (93.7 min). They feed Task 6's decisions and Task 7's numbers.

- [ ] **Step 7: Show the fallback once, end to end (acceptance criterion 4)**

The fixture tests prove the decision; this shows the whole path on the real runner, once, and is not left in CI. The disk guard is forced to fail from `host_facts`, which no contract test reads, so the throwaway PR still reaches the VM job:

```bash
git switch -c ci/arm64-fallback-demo
sed -i 's/free_bytes=shutil.disk_usage(directory).free/free_bytes=0/' scripts/vm_native.py
git diff --stat   # exactly one line changed in scripts/vm_native.py
git commit -am "demo: force the native phase to fall back"
git push -u origin ci/arm64-fallback-demo
gh pr create --draft --base main --title "demo: native provisioning fallback (do not merge)" \
  --body "Throwaway. Forces the disk guard to fail so the fallback can be seen once end to end. Close without merging."
```

Expected in that run's aarch64 job: an annotation titled `Native provisioning skipped` carrying `NativeUnavailable: 0.0 GiB free, 10 GiB needed`; `result.json` with `provisioning: in-guest` and that `fallback_reason`; the job passing the slow way (about 94 minutes); and the same 41 assertions (Step 5). Then close it and return, leaving the remote branch for the user to delete:

```bash
gh pr close <demo-pr-number>
git switch ci/arm64-vm-native-provision
```

---

### Task 6: Boot and install tuning, one change at a time

Only after Task 5 shows `provisioning: native` and the same 41 assertions. Each change below is its own commit and its own CI run, and all three touch the disposable test fixture only (never the product). **Keep a change only if** the aarch64 step is at least 3 minutes shorter, the 41 assertions are identical, and there is no new fallback; otherwise `git revert` it and say so in the PR. Each diff below was applied and its contract test run (red, then green) on a scratch copy of the repository.

**Files:**
- Modify: `scripts/vm_native.py`, `scripts/test-vm.py`, `scripts/test-vm-fixture.py`

**Interfaces:**
- Produces: `chroot_script(partition, mount, manifest, policy, extras=(), masks=())`, where `extras` is `((source_path, path_in_guest), ...)` installed 0644 and `masks` is systemd unit names linked to `/dev/null`. Both persist into the booted guest on purpose.

#### 6a. dpkg without per-file fsync

- [ ] **Step 1: Add the failing test.** Apply to `scripts/test-vm-fixture.py`:

```diff
--- a/scripts/test-vm-fixture.py
+++ b/scripts/test-vm-fixture.py
@@ -301,6 +301,43 @@
     return failures


+def check_native_tuning(native):
+    failures = []
+    with tempfile.TemporaryDirectory(prefix="decklock-native-") as directory:
+        bin_dir, log = stubs(directory)
+        (bin_dir / "ln").write_text(f'#!/bin/bash\necho "ln $*" >> "{log}"\n')
+        (bin_dir / "ln").chmod(0o755)
+        try:
+            script = native.chroot_script("/dev/loop7p1", Path(directory) / "root", "/x/ubuntu.env",
+                                          Path(directory) / "policy-rc.d",
+                                          extras=(("/x/dpkg-fast.cfg", "etc/dpkg/dpkg.cfg.d/decklock-vm-fast"),),
+                                          masks=("snapd.seeded.service",))
+        except TypeError:
+            return ["chroot_script does not take the tuning options yet"]
+        env = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}")
+        subprocess.run(["bash", "-c", script], env=env, capture_output=True, text=True, timeout=20)
+        calls = log.read_text().splitlines()
+        order = next((i for i, call in enumerate(calls) if call.startswith("chroot")), len(calls))
+        if not any(call.startswith("install -m 0644 /x/dpkg-fast.cfg") and call.endswith("decklock-vm-fast")
+                   for call in calls[:order]):
+            failures.append("the dpkg setting was not installed before the chroot")
+        if not any(call.startswith("ln -sf /dev/null") and call.endswith("snapd.seeded.service")
+                   for call in calls[:order]):
+            failures.append("the unit was not masked before the chroot")
+        if any("decklock-vm-fast" in call or call.startswith("ln ") for call in calls[order:]):
+            failures.append("a fixture setting was removed again; it must stay in the guest")
+    with tempfile.TemporaryDirectory(prefix="decklock-native-") as directory:
+        work = Path(directory)
+        (work / "base.img").write_bytes(b"")
+        runner = FakeRun()
+        native.provision(work / "base.img", work / "ubuntu.env", work, work / "provision.log",
+                         run=runner, geteuid=lambda: 1000, settle=lambda _: None, leftover=lambda mount: [])
+        script = next(command[-1] for command in runner.commands if "unshare" in command)
+        if "decklock-vm-fast" not in script:
+            failures.append("provision does not install the dpkg setting")
+    return failures
+
+
 def main():
     failures = []

@@ -398,7 +435,8 @@
     failures.extend(problems)
     if native is not None:
         for check in (check_provisioning_script, check_native_preconditions, check_chroot_script,
-                      check_native_orchestration, check_native_fallback, check_mounts_under):
+                      check_native_orchestration, check_native_fallback, check_mounts_under,
+                      check_native_tuning):
             failures.extend(check(native))

     failures.extend(check_native_wiring())
```

Run: `python3 scripts/test-vm-fixture.py`
Expected: `FAIL: chroot_script does not take the tuning options yet`

- [ ] **Step 2: Implement.** Apply to `scripts/vm_native.py`:

```diff
--- a/scripts/vm_native.py
+++ b/scripts/vm_native.py
@@ -50,14 +50,17 @@
         '$SYNC_AND_INSTALL $HARNESS_PACKAGES\n')


-def chroot_script(partition, mount, manifest, policy):
+def chroot_script(partition, mount, manifest, policy, extras=(), masks=()):
     """Runs under `unshare --mount --propagation private`: mount, prepare, install, restore.

     The EXIT trap is installed before anything is prepared and undoes only what was done,
     so the guest's own files come back whether the install succeeds or any step fails.
-    The mounts need no undoing: they die with the namespace.
+    The mounts need no undoing: they die with the namespace. `extras` (source, path in
+    the guest) and `masks` (systemd units) are fixture tuning that deliberately stays.
     """
     q = shlex.quote
+    tuning = ''.join(f'install -m 0644 {q(str(source))} "$root/{target}"\n' for source, target in extras)
+    tuning += ''.join(f'ln -sf /dev/null "$root/etc/systemd/system/{unit}"\n' for unit in masks)
     return (
         'set -euo pipefail\n'
         f'root={q(str(mount))}\n'
@@ -74,6 +77,7 @@
         'mount -t proc proc "$root/proc"\n'
         f'install -m 0755 {q(str(policy))} "$root/usr/sbin/policy-rc.d"\n'
         f'cp {q(str(manifest))} "$root{ENV_IN_GUEST}"\n'
+        + tuning +
         'mv "$root/etc/resolv.conf" "$root/etc/resolv.conf.decklock-orig"\n'
         'moved=1\n'
         'cp /etc/resolv.conf "$root/etc/resolv.conf"\n'
@@ -163,6 +167,8 @@
     raw = work / 'disk.raw'
     policy = work / 'policy-rc.d'
     policy.write_text('#!/bin/sh\nexit 101\n')
+    dpkg_cfg = work / 'dpkg-fast.cfg'
+    dpkg_cfg.write_text('force-unsafe-io\n')
     mount = Path(tempfile.mkdtemp(prefix='decklock-root-'))
     with Path(log_path).open('w') as log:
         host = Host(log, run, geteuid)
@@ -179,7 +185,9 @@
                     raise subprocess.CalledProcessError(4, ['e2fsck', '-fy', partition])
                 host(['resize2fs', partition], timeout=600)
                 host(['unshare', '--mount', '--propagation', 'private', 'bash', '-c',
-                      chroot_script(partition, mount, manifest, policy)], timeout=1800)
+                      chroot_script(partition, mount, manifest, policy,
+                                    extras=((dpkg_cfg, 'etc/dpkg/dpkg.cfg.d/decklock-vm-fast'),))],
+                     timeout=1800)
             finally:
                 host(['sync'], timeout=120, check=False)
                 host(['losetup', '-d', loop], timeout=60)
```

Run: `python3 scripts/test-vm-fixture.py` then `scripts/check`
Expected: `PASS: cloud-init readiness and native provisioning contracts`, all steps `PASS`. (The test also asserts that the cleanup trap never removes the tuning and that `provision` actually installs it.)

- [ ] **Step 3: Commit, measure, decide.**

```bash
git add scripts/vm_native.py scripts/test-vm-fixture.py
git commit -m "ci: skip per-file fsync in the aarch64 guest's dpkg"
git push
```

Then repeat Task 5 Steps 3-6 and apply the keep rule.

#### 6b. Mask background services in the guest

- [ ] **Step 1: Add the failing assertion.** Apply to `scripts/test-vm-fixture.py`:

```diff
--- a/scripts/test-vm-fixture.py
+++ b/scripts/test-vm-fixture.py
@@ -335,6 +335,8 @@
         script = next(command[-1] for command in runner.commands if "unshare" in command)
         if "decklock-vm-fast" not in script:
             failures.append("provision does not install the dpkg setting")
+        if "snapd.seeded.service" not in script:
+            failures.append("provision does not mask the background services")
     return failures
```

Run: `python3 scripts/test-vm-fixture.py`
Expected: `FAIL: provision does not mask the background services`

- [ ] **Step 2: Implement.** Apply to `scripts/vm_native.py`:

```diff
--- a/scripts/vm_native.py
+++ b/scripts/vm_native.py
@@ -30,6 +30,10 @@
 REQUIRED_TOOLS = ('qemu-img', 'losetup', 'lsblk', 'unshare', 'chroot',
                   'growpart', 'e2fsck', 'resize2fs')
 ENV_IN_GUEST = '/tmp/decklock-target.env'
+# Background work that competes with the suites in an emulated guest and proves nothing.
+MASKED_UNITS = ('snapd.seeded.service', 'snapd.service', 'snapd.socket', 'apt-daily.timer',
+                'apt-daily-upgrade.timer', 'apt-news.service', 'update-notifier-download.timer',
+                'motd-news.timer', 'man-db.timer')


 class NativeUnavailable(Exception):
@@ -186,7 +190,8 @@
                 host(['resize2fs', partition], timeout=600)
                 host(['unshare', '--mount', '--propagation', 'private', 'bash', '-c',
                       chroot_script(partition, mount, manifest, policy,
-                                    extras=((dpkg_cfg, 'etc/dpkg/dpkg.cfg.d/decklock-vm-fast'),))],
+                                    extras=((dpkg_cfg, 'etc/dpkg/dpkg.cfg.d/decklock-vm-fast'),),
+                                    masks=MASKED_UNITS)],
                      timeout=1800)
             finally:
                 host(['sync'], timeout=120, check=False)
```

Run: `python3 scripts/test-vm-fixture.py` then `scripts/check`
Expected: `PASS`, all steps `PASS`.

- [ ] **Step 3: Commit, measure, decide.**

```bash
git add scripts/vm_native.py scripts/test-vm-fixture.py
git commit -m "ci: mask background services in the aarch64 test guest"
git push
```

Then Task 5 Steps 3-6. Also confirm the guest reached SSH and `cloud-init status` finished: masking `snapd.seeded.service` is the entry most likely to stall first boot. If boot regresses, revert and drop the `snapd` entries only, then re-measure.

#### 6c. Do not flush the disposable disk

- [ ] **Step 1: Add the failing check.** Apply to `scripts/test-vm-fixture.py`:

```diff
--- a/scripts/test-vm-fixture.py
+++ b/scripts/test-vm-fixture.py
@@ -268,6 +268,7 @@
             ("vm_native.try_native(", "test-vm.py does not call vm_native.try_native"),
             ("report['provisioning']", "the report does not record how the guest was provisioned"),
             ("fallback_reason", "a fallback does not record its reason"),
+            ("cache=unsafe", "the native disk does not skip host flushes"),
             ("::warning", "a fallback does not raise a visible warning")):
         if needle not in source:
             failures.append(why)
```

Run: `python3 scripts/test-vm-fixture.py`
Expected: `FAIL: the native disk does not skip host flushes`

- [ ] **Step 2: Implement.** Apply to `scripts/test-vm.py`:

```diff
--- a/scripts/test-vm.py
+++ b/scripts/test-vm.py
@@ -179,7 +179,7 @@
             raw, reason = vm_native.try_native(
                 base, ROOT / 'scripts/vm/distros' / f'{args.target}.env', work, logs, args.arch)
             if raw is not None:
-                disk = f'file={raw},if=virtio,format=raw'
+                disk = f'file={raw},if=virtio,format=raw,cache=unsafe'
                 report['provisioning'] = 'native'
             else:
                 report['fallback_reason'] = reason
```

Run: `python3 scripts/test-vm-fixture.py` then `scripts/check`
Expected: `PASS`, all steps `PASS`. The disk is a disposable copy; a host crash discards the run anyway.

- [ ] **Step 3: Commit, measure, decide.**

```bash
git add scripts/test-vm.py scripts/test-vm-fixture.py
git commit -m "ci: do not flush the disposable guest disk"
git push
```

Then Task 5 Steps 3-6 and the keep rule.

---

### Task 7: Bounds, documentation and landing

**Files:**
- Modify: `scripts/test-vm.py`, `scripts/test-vm-fixture.py`, `.github/workflows/checks.yml`, `docs/vm-testing.md`, `docs/testing.md`, `docs/ROADMAP.md`, `.specs/project/STATE.md`, `.specs/features/arm64-vm-gate-speed/spec.md`

- [ ] **Step 1: Collect the numbers the texts below use.** From the Task 5/6 runs of the head that will merge: `RUN_ID` (the run), `STEP_MIN` (minutes of the aarch64 step "Verify candidate and run disposable VM" in the slowest passing run), `KEPT` and `REVERTED` (which of 6a-6c stayed or were reverted), and later `PR_NUMBER`. The baseline is run 36897087368 at 93.7 minutes.

- [ ] **Step 2: Bound the guest phase more tightly on the native path.** Add the failing check, apply to `scripts/test-vm-fixture.py`:

```diff
--- a/scripts/test-vm-fixture.py
+++ b/scripts/test-vm-fixture.py
@@ -269,6 +269,8 @@
             ("report['provisioning']", "the report does not record how the guest was provisioned"),
             ("fallback_reason", "a fallback does not record its reason"),
             ("cache=unsafe", "the native disk does not skip host flushes"),
+            ("(600 if report['provisioning'] == 'native' else 1200)",
+             "the guest phase is not bounded more tightly on the native path"),
             ("::warning", "a fallback does not raise a visible warning")):
         if needle not in source:
             failures.append(why)
```

Run: `python3 scripts/test-vm-fixture.py`
Expected: `FAIL: the guest phase is not bounded more tightly on the native path`

Then apply to `scripts/test-vm.py`:

```diff
--- a/scripts/test-vm.py
+++ b/scripts/test-vm.py
@@ -249,7 +249,7 @@
                 print('Guest ready; installing dependencies and exercising the candidate.', flush=True)
                 with (logs / 'guest.log').open('w') as guest_log:
                     result = subprocess.run([*ssh, f'DECKLOCK_VM_SLOWDOWN={slowdown} bash /root/decklock-fixture/setup.sh {args.target}'],
-                                            stdout=guest_log, stderr=subprocess.STDOUT, timeout=1200 * slowdown)
+                                            stdout=guest_log, stderr=subprocess.STDOUT, timeout=(600 if report['provisioning'] == 'native' else 1200) * slowdown)
                 if result.returncode:
                     raise RuntimeError(f'Guest test failed ({result.returncode}); see guest.log')
                 report['status'] = 'passed'
```

Run: `python3 scripts/test-vm-fixture.py` then `scripts/check`
Expected: `PASS`, all steps `PASS`. The measured guest phase is about 13 minutes of suites plus the candidate install, against a bound of 80 (600 s at slowdown 8); only the ceiling for a hang changes, never what is asserted.

- [ ] **Step 3: Set the job timeout from the bounds.** On the native path the script's own bounds add up to 76 min of host provisioning (the sum of its per-command timeouts), 32 min of boot, 10 min of copying the candidate, 80 min of guest phase and 4 min of evidence transfer, 202 minutes, so the job ceiling goes just above that. In `.github/workflows/checks.yml` replace:

```
    # Above test-vm.py's own scaled bounds (boot 32 min, guest 160 min), so a
    # stuck guest still ends in its log rather than a cancelled job. Measured:
    # a passing run takes about 100 min, 10 of them booting.
    timeout-minutes: 300
```

with (substitute `STEP_MIN` and `RUN_ID` from Step 1):

```
    # Above the sum of test-vm.py's own bounds on the native path (host provisioning
    # 76 min, boot 32, copying 10, guest phase 80, evidence 4 = 202), so a stuck guest
    # still ends in its log rather than a cancelled job. A passing run takes about
    # STEP_MIN min (run RUN_ID), down from about 94 before the harness packages were
    # installed on the host. The timeout only caps a hang.
    timeout-minutes: 210
```

Run: `python3 scripts/test-vm-fixture.py`. Expected: `PASS` (the workflow checks still require all ten gates and no flag on the KVM jobs).

- [ ] **Step 4: Documentation.** Four edits, each with its exact text.

(a) `docs/vm-testing.md`: after the paragraph ending `PAM and compositors, not behavior on real ARM hardware.` add a blank line and:

```
CI also passes `--provision-native`. Before the guest boots, the runner installs the
harness packages (the same `PRE_SYNC` and `SYNC_AND_INSTALL $HARNESS_PACKAGES` commands
`setup.sh` runs, read from the same `distros/<target>.env`) into a raw copy of the pinned
image, in a chroot of the same architecture, so they are not installed under emulation.
That step was about 70 of the gate's 94 minutes. It needs the host and guest
architectures to match, root or passwordless sudo, and about 10 GiB free. All mounting
happens in a private mount namespace on a mount point outside the work directory, and the
guest's own files are restored by a trap that exists before anything is prepared. The
candidate install, the reinstall-preserves-configuration check, the PAM fixtures and
every suite still run in the booted guest, and `setup.sh` is unchanged. If the step cannot
run or fails, the run warns (`::warning::`), records `fallback_reason` and `provisioning:
in-guest` in `result.json` and installs inside the guest as before. `provision.log` in the
evidence records each host command and its duration. A passing run now takes about
STEP_MIN minutes. Kept after measurement: KEPT. Tried and reverted: REVERTED.
```

(b) `docs/testing.md`, the `aarch64 VM gate` row: replace `with every bounded wait scaled by 8; about 100 minutes. Fedora arm64 has no VM target yet |` with `with every bounded wait scaled by 8, after the harness packages are installed on the host (`--provision-native`); about STEP_MIN minutes, down from about 94. Fedora arm64 has no VM target yet |`.

(c) `docs/ROADMAP.md`, the Architectures bullet: replace `so a run takes about 100 minutes)` with `so a run took about 94 minutes until the harness packages were installed on the host before boot; it now takes about STEP_MIN)` and, in the Portuguese half of the same bullet, replace `então uma execução leva cerca de 100 minutos)` with `então uma execução levava cerca de 94 minutos até os pacotes do harness passarem a ser instalados no host antes do boot; agora leva cerca de STEP_MIN)`.

(d) `.specs/project/STATE.md`: append (substitute the five named values):

```
2026-10-03 aarch64 VM gate sped up (PR #PR_NUMBER): the emulated Ubuntu arm64 gate took
93.7 min (run 36897087368): about 7 booting, about 70 installing 385 packages under TCG
and about 13 in the suites, which the guest journal showed after an earlier estimate had
put the suites at 51. `--provision-native` now installs HARNESS_PACKAGES into a raw copy
of the pinned image on the host, in a same-architecture chroot, before the guest boots;
the candidate, the reinstall check, the PAM fixtures and every suite still run in the
booted guest and setup.sh is unchanged. A failure warns, records `fallback_reason` and
takes the in-guest path. All mounting runs under `unshare --mount --propagation private`:
a bind of /dev under systemd's shared mounts would have propagated to the host's
/dev/pts. The mount point is outside the work directory and removed only with rmdir. The
guest phase is bounded at 600 s x slowdown on this path and the job timeout is 210 min,
the sum of the script's own bounds. Measured STEP_MIN min (run RUN_ID) with the same 41
assertions as the baseline. Kept: KEPT. Reverted: REVERTED. Untested: real ARM
hardware, and the hosted runner beyond what these runs showed. Next: the controller
reconnection fix, then the 0.3.3 release.
```

- [ ] **Step 5: Spec status.** In the spec set the status line to `Status: implemented; measured result recorded in STATE.md.`

- [ ] **Step 6: Final gate and landing.**

```bash
scripts/check --all
git add -A scripts .github docs .specs
git status --short   # only the files listed above
git commit -m "ci: bound the native guest phase and record the measured gate time"
git push
gh pr ready
```

Wait for every required check including `DeckLock checks`, then merge through the protected flow with `gh pr merge --squash`. The PR body must state the baseline and the measured durations, that the 41 assertions are unchanged, that no required check, workflow condition or assertion was removed or loosened, the fallback demonstration, and what remains untested (real ARM hardware, the runner beyond these runs). After it merges, the held work resumes: the controller reconnection fix, then the 0.3.3 release PR, then the tag.
