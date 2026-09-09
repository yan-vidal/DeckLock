# Real Wayland/PAM integration VM

The guest uses the installed Arch package, unmodified Sway and real Linux-PAM
modules. It complements the small mock protocol and unit tests; it does not
replace them or establish compatibility with every compositor and PAM policy.

Run from the repository on an x86_64 Linux host with KVM, at least 3 GiB available
RAM, QEMU, qemu-img, genisoimage, curl and OpenSSH:

```sh
python3 scripts/test-vm.py --package /path/to/decklock-0.2.0-1-x86_64.pkg.tar.zst
```

Obtain the candidate from the `arch-package` artifact of the intended CI run.
Verify its SHA256SUMS before running. The runner records the package hash; the
installed package includes its original BUILD-INFO.json source provenance.

The runner uses 2 virtual CPUs and 2048 MiB RAM. It refuses to start below 3072 MiB
available host memory and terminates only its own VM if available memory stays
below 768 MiB or full memory pressure exceeds 5% for three consecutive 3-second samples. KVM is required; it never
silently falls back to CPU-heavy software emulation. Boot, SSH, provisioning and
test waits are bounded. No application compilation takes place inside the VM.

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

Because runuser bypasses the initial login, provisioning initializes the user-owned
tally with the real faillock utility before running the locker. No tally records
are fabricated.

The ordinary test uses the default `login` PAM service shipped by Arch. A separate
**guest-only** service loads real pam_unix and pam_faillock with a short known
lockout policy to test lockout messages and expiry without waiting ten minutes.
This fixture is not installed by the DeckLock package and does not change the
application's default service or the host's policy.

Evidence is written under `target/vm-logs`: host memory samples, package/image
hashes, guest package versions and PAM configuration, compositor/client traces,
status notices and explicit assertion results. The temporary VM disk and SSH key
are deleted after shutdown, including failures. Logs may contain the public test
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
