#!/bin/bash
# Guest-only provisioning. Refuse execution on the developer's system.
set -euo pipefail
[[ $(cat /etc/decklock-test-vm 2>/dev/null) == disposable-qemu-fixture ]]
[[ $(systemd-detect-virt) == kvm || $(systemd-detect-virt) == qemu ]]
[[ $EUID == 0 ]]
export LC_ALL=C
trap 'journalctl -b --no-pager > /var/tmp/decklock-evidence/journal.log' EXIT
cloud-init status --wait
pacman -Syu --noconfirm --needed sway wtype python-gobject gtk4 gtk4-layer-shell gstreamer gst-plugins-base gst-plugins-good gst-libav pam grim at-spi2-core
pacman -U --noconfirm /root/decklock-fixture/*.pkg.tar.zst
useradd --create-home --shell /bin/bash locktest
# Public fixture credentials, confined to this disposable guest.
printf 'locktest:DeckLock-test-42\n' | chpasswd
# runuser bypasses login authentication: initialize the user-owned tally as a
# privileged initial login normally would, without inventing binary tally data.
faillock --user locktest --reset
runtime=/run/user/$(id -u locktest)
install -d -o locktest -g locktest -m 700 "$runtime"
install -d -o locktest -g locktest /var/tmp/decklock-evidence
cp /root/decklock-fixture/*.py /var/tmp/decklock-evidence/
chmod a+r /var/tmp/decklock-evidence/*.py
pacman -Q > /var/tmp/decklock-evidence/packages.txt
cp /etc/pam.d/{login,system-local-login,system-login,system-auth} /var/tmp/decklock-evidence/
sha256sum /usr/bin/decklock > /var/tmp/decklock-evidence/binary.sha256
cp /usr/share/doc/decklock/BUILD-INFO.json /var/tmp/decklock-evidence/
# A fixture service only, never included in the application package.
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
pacman -U --noconfirm /root/decklock-fixture/*.pkg.tar.zst
sha256sum --check /var/tmp/decklock-evidence/config-before.sha256
printf 'PASS: candidate reinstallation preserves user configuration\n'
runuser -u locktest -- env HOME=/home/locktest XDG_RUNTIME_DIR="$runtime" dbus-run-session -- python3 /var/tmp/decklock-evidence/exercise.py
