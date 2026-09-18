#!/bin/bash
# Waits for cloud-init and decides whether this guest can be tested.
#
# Contract, exercised by scripts/test-vm-fixture.py:
#   exit 0  the guest finished provisioning and is usable
#   exit 1  cloud-init failed; the guest is not a valid test environment
#
# cloud-init separates "it did not work" from "something recoverable happened":
# a run that finishes after a recoverable error still prints "status: done" and
# exits 2. That guest boots, installs packages and runs the test fine, so exit 2
# must not take the whole run down. Everything it reports is written to $1, which
# the caller collects as evidence, because a status nobody recorded is how one of
# these runs failed with no way to tell why.
set -uo pipefail
detail=${1:-/var/tmp/cloud-init-status.txt}
cloud-init status --wait
code=$?
{
    printf 'cloud-init status --wait exited %s\n' "$code"
    cloud-init status --long 2>&1
} > "$detail"
case $code in
    0) exit 0 ;;
    2)
        echo "cloud-init finished with a recoverable error (exit 2); continuing. Detail: $detail" >&2
        exit 0
        ;;
    *) exit 1 ;;
esac
