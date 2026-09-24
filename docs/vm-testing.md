# Real Wayland/PAM integration VM

The guest uses that distribution's installed package, unmodified Sway and real
Linux-PAM modules. It complements the small mock protocol and unit tests; it does
not replace them or establish compatibility with every compositor and PAM policy.
Each manifest's `EXTRA_COMPOSITORS` (labwc and Wayfire on every target) then
repeats the lock boundary through `vm/compositor.py`: input isolation, a real PAM
denial, one unlock, input restored and a killed locker leaving the session
locked. Faillock, account policy and X11 stay on Sway, because they do not depend
on the compositor.

The guest has no GPU. labwc runs headless with wlroots' pixman renderer. Wayfire
renders only with GLES, and wlroots requires a DRM render node for it, which the
emulated display card lacks. Provisioning loads `vgem`, a kernel DRM device with
no hardware behind it, before the package sync (a kernel upgrade would otherwise
remove the running kernel's modules). Wayfire then runs with
`WLR_RENDER_DRM_DEVICE` on vgem's render node, Mesa keeps buffers in memory
(`GBM_ALWAYS_SOFTWARE`) and draws them with llvmpipe, and
`WLR_RENDERER_ALLOW_SOFTWARE` lets wlroots accept that. The test user joins the
`render` and `video` groups, which Mesa needs to open vgem's nodes. This proves
the lock boundary on Wayfire's code, not on real GPU drivers.

Hyprland was tried the same way, in its headless-only mode, and does not start:
its Aquamarine backend opens a seat and needs a GBM allocator from a real GPU
even for headless outputs ("no allocator available"). niri and COSMIC have no
headless mode, and river is not packaged on Ubuntu 26.04. These stay
protocol-only.
After the locker assertions, the same candidate runs as a greetd greeter inside
headless Cage. greetd runs as a transient system service, as the distributions'
`greetd.service` does: started from the SSH login it would sit inside that
logind session, and pam_systemd would register neither the greeter nor the user
sessions or give them `XDG_RUNTIME_DIR`. The fixture checks that runtime
directory, a denied password, an accepted login, the selected Wayland session
running as the authenticated account, a return to the greeter after logout, and
login as a second account chosen with the arrow keys. The guest uses public test
credentials and its own Wayland socket.

One target per distribution, each with its own pinned cloud image and its own
candidate package:

| Target | Image | Candidate |
|---|---|---|
| `arch` | Arch cloud image | `.pkg.tar.zst` |
| `fedora` | Fedora Cloud Base Generic 43 | `.rpm` |
| `ubuntu` | Ubuntu 26.04 server cloudimg | `.deb` |

Sway 1.11 and wtype 0.4 are packaged on all three, so the assertions in
`scripts/vm/exercise.py` are shared and unchanged. Only provisioning varies, as
data in `scripts/vm/distros/<target>.env`: the package manager commands, the
harness package names, the reinstall command and which `/etc/pam.d` files to
record. A plain install of an already-installed version is a no-op on dnf and
apt, so each target names its own reinstall command; otherwise the
reinstall-preserves-configuration assertion would pass without reinstalling.

Neither Fedora nor Ubuntu ships a compositor implementing `ext-session-lock-v1`,
so each guest installs Sway. That is a property of the test rig, not a
recommendation: it is how the lock path is exercised at all on those guests.

Run from the repository on an x86_64 Linux host with KVM, at least 3 GiB available
RAM, QEMU, qemu-img, genisoimage, curl and OpenSSH:

```sh
python3 scripts/test-vm.py --target arch --package /path/to/decklock-0.2.0-1-x86_64.pkg.tar.zst
```

`qemu-img` must be 6.0 or newer. A much older copy ahead of the system one on
`PATH`, for instance from a bundled Android SDK, is refused by name rather than
left to fail later in a way that looks like a guest problem.

Obtain the candidate from that target's `<target>-package` artifact of the
intended CI run; the target and the package suffix must agree or the runner
refuses to start.
Verify its SHA256SUMS before running. The runner records the package hash; the
installed package includes its original BUILD-INFO.json source provenance.

The runner uses 2 virtual CPUs and 2048 MiB RAM. It refuses to start below 3072 MiB
available host memory and terminates only its own VM if available memory stays
below 768 MiB or full memory pressure exceeds 5% for three consecutive 3-second samples. KVM is required; it never
silently falls back to CPU-heavy software emulation. Boot, SSH, provisioning and
test waits are bounded. No application compilation takes place inside the VM.

`--arch aarch64` runs the Ubuntu arm64 cloud image (pinned by SHA256 like the
others) on QEMU's `virt` machine with UEFI firmware from `qemu-efi-aarch64`. The
hosted arm64 runners expose no `/dev/kvm`, so CI passes `--allow-emulation`: the
guest runs under TCG with 4 virtual CPUs, and the runner hands the guest a
slowdown factor of 8 that multiplies every host timeout and every bounded wait
in the guest scripts (deadlines, subprocess timeouts, settle delays). The
fixture's faillock `unlock_time` scales with it (96 s instead of 12 s): typing
the correct password took about 12 s in the emulated guest, so an unscaled
lockout ran out before PAM saw it. That still expires before the UI's
whole-minute estimate reaches zero. Polling intervals are not scaled, and the
assertions are the same ones the x86_64 guests run. The X11 part waits until
the locker holds the keyboard and pointer grabs, probed as another client would
(a grab someone else holds is refused), rather than for a fixed delay: DeckLock
refuses passwords until it holds both, and typing after a fixed second (eight
under emulation) lost the keys there. The same probe checks the grabs are
released when that locker is killed. Without `--allow-emulation` the runner still
refuses a guest it cannot accelerate. The report records the accelerator and
the factor. Fedora has no aarch64 target yet; its arm64 package is covered by
the package contract only. Emulation proves the arm64 build under a real kernel,
PAM and compositors, not behavior on real ARM hardware.

An official Arch cloud image is pinned by version and SHA256. The disk is a
fresh copy-on-write overlay and cloud-init installs a one-run SSH key. QEMU uses
user-mode networking with SSH bound only to a random localhost port. The guest
receives the candidate and the test scripts, not the host home, desktop sockets,
controller socket, PAM files, SSH agent or physical devices. The guest account's
password is public test data, not a developer credential. Only guest setup runs
as root; Sway, the probe and DeckLock run as an ordinary user.

Sway runs with its headless output and software renderer. `wtype` supplies input
through the virtual-keyboard protocol. A separate GTK window records key values
before and after locking and must receive nothing while the session is locked.
Wayland protocol traces independently confirm ownership and unlock requests.
AT-SPI reads the status label through the ordinary accessibility interface; the
application contains no test authentication override or unlock backdoor.

The test runs inside a real user manager rather than a `dbus-run-session`
bus: provisioning enables linger, waits for logind's session bus, and runs
against it. This matters on Fedora, the one target enforcing SELinux. Under
`dbus-run-session` the bus daemon runs as `unconfined_dbusd_t` and must exec
the AT-SPI launcher itself, a transition to `gnome_atspi_t` that policy does not
authorize for the `unconfined_r` role, so execution is denied. With a user
manager, `org.a11y.Bus` activates through its `SystemdService=` line, as it does
on a desktop login. SELinux stays enforcing, so the gate keeps exercising the
environment Fedora users have. That denial is recorded as a `SELINUX_ERR`, not
an AVC, so searching audit records for AVC denials alone does not find it.

Because runuser bypasses the initial login, provisioning initializes the user-owned
tally with the real faillock utility before running the locker. No tally records
are fabricated.

The ordinary test uses the default `decklock` service installed by the candidate
package, so each target exercises its own authentication stack: `system-auth` on
Arch and Fedora, `common-auth` and `common-account` on Ubuntu.

Whether that stack counts failed passwords is the distribution's policy, and
they differ. Only Arch wires `pam_faillock` into its default stack; Fedora keeps
it an opt-in `authselect` feature and Ubuntu omits it. Provisioning reads this
from configuration and the test asserts PAM's behavior against it -- exactly one
recorded failure on Arch, none on Fedora or Ubuntu -- so the gate verifies each
distribution's real policy rather than assuming Arch's. The expectation is never
derived from the observed tally, which would make the check tautological. A
consequence for users: with a stock configuration the lock-screen lockout notice
can only appear on Arch.

A separate
**guest-only** service loads real pam_unix and pam_faillock with a short known
lockout policy to test lockout messages and expiry without waiting ten minutes.
This fixture is not installed by the DeckLock package and does not change the
application's default service or the host's policy.

Evidence is written under `target/vm-logs`: the target name, host memory samples,
package/image hashes, guest package versions, the manifest used, the installed
`/etc/pam.d/decklock` and the distribution's own PAM configuration, compositor/client traces,
status notices, explicit assertion results, the guest's SELinux mode with its
audit denials, the failure-counting policy read from the stack, and cloud-init's
own status and log. A guest whose cloud-init finishes with a recoverable error
(exit 2, still "done") is provisioned and tested like any other; only a
cloud-init that failed outright makes the guest unusable, and both outcomes are
recorded rather than inferred. VM evidence also includes greetd's log,
selected-session markers and `greeter-assertions.txt`; a missing greeter
assertion fails that target's VM job. The temporary VM disk and SSH key are
deleted after shutdown, including failures. They live
under `.deps/vm-cache` rather than `/tmp`: the copy-on-write overlay grows with
every guest write, and `/tmp` is tmpfs by default on Arch and Fedora, where that
growth would be host memory rather than disk. Logs may contain the public test
password's key events, so never adapt this fixture to personal credentials.

The image checksum pins the base, not every guest runtime package: Arch mirrors
are updated. Package versions are recorded, and unavailable images/dependencies
are failures. Updating the image pin is a reviewed maintenance change.

Remaining boundaries include real GPU rendering, physical input/controllers,
actual suspend/hibernate and recovery on the user's compositor. A VM pass does
not prove those or constitute a security audit.

References: [Arch image archive](https://geo.mirror.pkgbuild.com/images/v20260901.583572/),
[cloud-init NoCloud](https://docs.cloud-init.io/en/latest/reference/datasources/nocloud.html),
[wtype](https://github.com/atx/wtype).
