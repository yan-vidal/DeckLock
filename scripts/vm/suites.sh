# Which suites this guest runs. Sourced by setup.sh: a list and one predicate, no effects.
# Unset or empty means every suite, in this order, as a KVM guest runs them. An emulated
# guest is too slow for all of them in turn, so test-vm.py --suites gives each of several
# guests some, and together they run each suite exactly once.
ALL_SUITES='package session compositors greeter'
suites="${DECKLOCK_VM_SUITES:-$ALL_SUITES}"
if [[ -z ${suites// /} ]]; then
    echo 'No suite selected' >&2
    return 2
fi
for suite in $suites; do
    if [[ " $ALL_SUITES " != *" $suite "* ]]; then
        printf 'Unknown suite: %s\n' "$suite" >&2
        return 2
    fi
done
runs() { [[ " $suites " == *" $1 "* ]]; }
