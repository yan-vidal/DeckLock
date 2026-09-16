#!/bin/bash
# Guest-only provisioning. Refuse execution on the developer's system.
set -euo pipefail
[[ $(cat /etc/decklock-test-vm 2>/dev/null) == disposable-qemu-fixture ]]
[[ $(systemd-detect-virt) == kvm || $(systemd-detect-virt) == qemu ]]
[[ $EUID == 0 ]]
fixture=/root/decklock-fixture
target="${1:?Pass the distribution target, matching a distros/<target>.env manifest}"
[[ -f $fixture/$target.env ]]
# Data only: package manager commands, package names, PAM files to record.
# Everything below is shared by every distribution.
# shellcheck source=/dev/null
source "$fixture/$target.env"
export LC_ALL=C
# Recorded on exit, so a failure is diagnosable without another CI round trip.
# A guest's mandatory access control is part of the environment being tested:
# SELinux denials go to the audit log, not the journal, so both are collected.
collect_evidence() {
    local out=/var/tmp/decklock-evidence
    journalctl -b --no-pager > "$out/journal.log" 2>&1 || true
    command -v getenforce >/dev/null && getenforce > "$out/selinux.txt" 2>&1 || true
    [[ -r /var/log/audit/audit.log ]] && grep -a 'avc:' /var/log/audit/audit.log > "$out/avc.log" 2>&1 || true
    command -v ausearch >/dev/null && ausearch -m AVC -ts boot > "$out/ausearch.log" 2>&1 || true
    true
}
trap collect_evidence EXIT
cloud-init status --wait
[[ -z $PRE_SYNC ]] || $PRE_SYNC
# shellcheck disable=SC2086
$SYNC_AND_INSTALL $HARNESS_PACKAGES
$INSTALL_CANDIDATE $fixture/$CANDIDATE_GLOB
useradd --create-home --shell /bin/bash locktest
# Public fixture credentials, confined to this disposable guest.
printf 'locktest:DeckLock-test-42\n' | chpasswd
# runuser bypasses login authentication: initialize the user-owned tally as a
# privileged initial login normally would, without inventing binary tally data.
faillock --user locktest --reset
runtime=/run/user/$(id -u locktest)
install -d -o locktest -g locktest -m 700 "$runtime"
install -d -o locktest -g locktest /var/tmp/decklock-evidence
cp $fixture/*.py /var/tmp/decklock-evidence/
chmod a+r /var/tmp/decklock-evidence/*.py
cp "$fixture/$target.env" /var/tmp/decklock-evidence/
$QUERY_PACKAGES > /var/tmp/decklock-evidence/packages.txt
# The distribution's own authentication stack, recorded rather than assumed.
for name in $PAM_EVIDENCE; do
    cp "/etc/pam.d/$name" /var/tmp/decklock-evidence/ 2>/dev/null ||
        printf 'absent on this distribution: %s\n' "$name" >> /var/tmp/decklock-evidence/pam-missing.txt
done
# The service the package installs and pam_service now defaults to.
cp /etc/pam.d/decklock /var/tmp/decklock-evidence/decklock.pam
sha256sum /usr/bin/decklock > /var/tmp/decklock-evidence/binary.sha256
cp /usr/share/doc/decklock/BUILD-INFO.json /var/tmp/decklock-evidence/
# Fixture services only, never included in the application package. pam_faillock
# and pam_unix exist on every target, so the policy itself needs no variation.
cat > /etc/pam.d/decklock-vm-test <<'PAM'
auth required pam_faillock.so preauth deny=2 unlock_time=12 fail_interval=900
auth [success=1 default=bad] pam_unix.so
auth [default=die] pam_faillock.so authfail deny=2 unlock_time=12 fail_interval=900
auth sufficient pam_faillock.so authsucc deny=2 unlock_time=12 fail_interval=900
account required pam_unix.so
PAM
cat > /etc/pam.d/decklock-vm-account <<'PAM'
auth required pam_unix.so
account required pam_deny.so
PAM
cp /etc/pam.d/decklock-vm-* /var/tmp/decklock-evidence/
runuser -u locktest -- decklock --config /home/locktest/candidate-config.toml config set idle_seconds 777
sha256sum /home/locktest/candidate-config.toml > /var/tmp/decklock-evidence/config-before.sha256
# A plain install of an already-installed version is a no-op on dnf and apt, so
# each target names its own reinstall command; otherwise this assertion would
# pass without reinstalling anything.
$REINSTALL_CANDIDATE $fixture/$CANDIDATE_GLOB
sha256sum --check /var/tmp/decklock-evidence/config-before.sha256
printf 'PASS: candidate reinstallation preserves user configuration on %s\n' "$target"
runuser -u locktest -- env HOME=/home/locktest XDG_RUNTIME_DIR="$runtime" dbus-run-session -- python3 /var/tmp/decklock-evidence/exercise.py
