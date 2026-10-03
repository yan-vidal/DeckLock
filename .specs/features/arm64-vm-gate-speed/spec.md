# Faster aarch64 VM gate: native provisioning before boot

Status: spec approved on 2026-10-02; amended on 2026-10-03 for the private mount namespace (R5, R8, Design); plan in plan.md.
Scope: CI and the VM harness only. No application code, package or release change.

## Problem

`vm-ubuntu-aarch64` (job "Real Wayland and PAM (Ubuntu aarch64)") is a required check
through `DeckLock checks`. It takes about 94 minutes; the x86_64 Ubuntu job takes 3. The
`strict_required_status_checks_policy` of `mainProtect` makes every merge force the next
PR to rerun it, and `release.yml` reruns it on the tag, so the cost is paid at least three
times per release.

Hosted arm64 runners have no `/dev/kvm` (actions/runner-images#14062 was closed as not
planned), so the guest runs under QEMU TCG. That is not changing and is not the target.

### Measured baseline

Run 36897087368 (`main`, head `db22748`, 2026-10-01), job step "Verify candidate and run
disposable VM": 93.7 min. Phase boundaries come from the guest journal and `guest.log`
(apt does not log to the journal, so the split inside the install phase is not measured):

| Phase | Guest clock | Duration |
|---|---|---|
| Kernel boot to first SSH login (cloud-init, snapd seeding) | 18:03 - 18:10 | ~7 min |
| Package installation: `apt-get update`, 385 packages, candidate install, reinstall | 18:11 - 19:22 | ~70 min |
| Suites: Sway/PAM/X11 (`exercise.py`), labwc and Wayfire (`compositor.py`), setup and greetd (`greeter.py`) | 19:22 - 19:35 | ~13 min |

Package installation is about three quarters of the job. Splitting the suites into parallel
jobs would save little (13 min of suites against roughly 10 min of boot and candidate
install repeated per shard) and is out of scope.

## Goals

- The aarch64 gate finishes in about 30 minutes (target, not yet measured).
- No assertion is removed, skipped, weakened or reordered, and no required check is relaxed.
- The guest keeps installing the latest Ubuntu archive packages on every run. No cache, no
  frozen image, no published artifact, no self-hosted runner.

## Non-goals

x86_64 jobs (KVM, 3 min); Fedora arm64 (no VM target); a real-KVM arm64 runner; sharding
the suite; changing what the VM proves.

## Requirements

- **R1 Native provisioning.** With `--provision-native`, before the VM boots, the harness
  installs `HARNESS_PACKAGES` into the guest disk on the host, at native speed, by running
  the same `PRE_SYNC` and `SYNC_AND_INSTALL` commands from `scripts/vm/distros/<target>.env`
  in a chroot of the work copy of the pinned image. It requires host architecture equal to
  guest architecture and root. The package list has a single source: the `.env` file.
- **R2 Everything else stays in the booted guest.** `scripts/vm/setup.sh` is unchanged.
  Installing the candidate, the reinstall-preserves-configuration check, the PAM fixtures
  and every suite run in the booted guest. Reinstalling already-installed harness packages
  there is an idempotent no-op, which is also what makes the slow path a valid fallback.
- **R3 Evidence.** `result.json` records `provisioning` as `native` or `in-guest`, and
  `fallback_reason` when it is `in-guest`. The native phase writes `provision.log` into the
  evidence directory. `packages.txt` keeps being read from inside the guest.
- **R4 Fallback is loud, never a false pass.** If native provisioning cannot run or fails,
  the job emits a visible `::warning::` annotation, records the reason, and continues on
  the in-guest path with the same assertions. A guest that fails its suite still fails the
  job.
- **R5 Containment.** The host only loop-attaches the harness's own work copy of the pinned
  image, never the base cache image or any host path. All mounting happens inside a private
  mount namespace (`unshare --mount --propagation private`), so nothing propagates to the
  host and nothing survives the namespace. The mount point is a fresh directory outside the
  work directory that is only ever removed with `rmdir`, so a recursive delete of the work
  directory cannot reach a mounted filesystem. The loop device is detached on every exit
  path, including failure and timeout. No host home, desktop socket, credential or device
  is exposed. The base image SHA256 is verified before use, as today.
- **R6 Fixture-only tuning, each measured.** After the native phase is working, boot and
  install tuning is added one change at a time, and kept only with a measured gain and no
  change in assertion behavior: masking `snapd` seeding, `apt-daily`, `apt-news`,
  `update-notifier`, `motd-news`; `man-db` auto-update off; dpkg `force-unsafe-io`;
  `cache=unsafe` on the disposable guest disk. These touch the test fixture only.
- **R7 Disk guard.** The native path checks free space on the host before converting the
  image and falls back (R4) when it is insufficient, rather than failing halfway.
- **R8 Tests.** Command construction, `.env` parsing, the fallback decision, the order of
  the host steps and the `result.json` fields are covered by the existing `vm-fixture` gate
  without root, with an injected runner. The chroot script's cleanup contract is covered by
  running the real script against logging stand-ins for `mount`, `chroot` and the file
  tools, including when a step fails early. The loop attach and the real chroot are covered
  by the CI job, which is the integration test.
- **R9 Timeout and docs.** On the native path the guest phase is bounded at 600 s times the
  slowdown instead of 1200, and `timeout-minutes` drops from 300 to just above the sum of
  `test-vm.py`'s own bounds, so a stuck guest still ends in its own log. The comment beside
  it states the sum and the measured duration with its run. `docs/vm-testing.md`,
  `docs/testing.md`, `docs/ROADMAP.md` and `.specs/project/STATE.md` describe the new flow
  and its limits. The timeout is only the ceiling for a hang; the speed-up is the typical run.

## Design

Host side (new module `scripts/vm_native.py`, called from `scripts/test-vm.py`; it lives
outside `scripts/vm/` because every file there is copied into the guest):

1. Preconditions: native architecture, root (or passwordless sudo), `losetup`, `lsblk`,
   `unshare`, `chroot`, `growpart`, `e2fsck`, `resize2fs`, free space (R7).
2. `qemu-img convert -O raw` of the verified base into the work directory; extend the file
   to the guest disk size (12 GiB, sparse); grow the root partition and filesystem
   (`cloudimg-rootfs`). A base image larger than the guest disk is refused rather than truncated.
3. `losetup --find --show -P` the copy, then run one script under
   `unshare --mount --propagation private`: mount the root, bind `/dev`, mount `/proc`,
   install a `policy-rc.d` that refuses service starts, swap in the host's `resolv.conf`,
   and run the install in a chroot.
4. In the chroot: `DEBIAN_FRONTEND=noninteractive LC_ALL=C` and the `.env` commands.
5. Fixture tuning from R6 (only the entries that earned their place).
6. The guest's own files are restored by an `EXIT` trap installed before anything is
   prepared, so it also runs when a step fails early. The mounts end with the namespace.
   `sync`, then detach the loop device, always (`try/finally`).
7. Boot QEMU from the resulting raw disk instead of a qcow2 overlay. The cloud-init seed,
   SSH, memory guard, slowdown scaling and evidence transfer are unchanged.

Provisioning happens outside the booted guest, so the pre-boot disk state is not the vendor
image any more. That is the same trust position as today, where the harness installs the
same packages on first boot; the base image is still pinned by SHA256 and the candidate is
still verified and installed from clean in the running guest.

## Acceptance criteria

1. The aarch64 `guest.log` contains exactly the 41 `PASS` lines of the baseline run
   (`baseline-pass.txt`), compared as sets. Lines that legitimately differ because the
   suite itself changed in the meantime are listed and explained, never silently dropped.
2. `result.json` shows `provisioning: native`, `status: passed`, `accelerator: tcg`.
3. The job step duration is reported with the baseline beside it. The target is about 30
   minutes; if it is not met, the measured split decides the next step rather than the target.
4. Forcing the native phase to fail (a deliberate test run) produces the warning, the
   `in-guest` result and a passing gate on the slow path. This is shown once, not left in CI.
5. `scripts/check --all` passes locally on the exact commit, and the full CI passes,
   including `DeckLock checks`.
6. No required check, workflow condition or assertion was removed or loosened; the diff
   states this explicitly.

## Risks and how they are handled

- The hosted arm64 runner may refuse loop devices, mounts or chroot. The first CI run is a
  probe of exactly that; if it fails the work stops and the finding is reported instead of
  improvising another mechanism.
- Packages whose maintainer scripts expect a running systemd behave differently in a
  chroot. `policy-rc.d` and the in-guest idempotent install plus the suites catch real
  breakage; anything that differs is recorded in `provision.log`.
- The hosted arm64 runner has a small disk. The raw image is sparse and measured; R7 falls
  back rather than failing.
- Nothing about the speed-up can be validated locally (no VMs here, and the hosted arm64
  runner is the only place the path runs), so each iteration costs one CI round trip.

## Order of work

1. This change, merged through the protected PR flow (one slow CI cycle).
2. The controller reconnection fix, then the 0.3.3 release PR, then the tag, all on the
   faster gate. The release is held until this lands, by the user's decision.
