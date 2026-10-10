# ARM VM preparation experiment

The user asked whether parallel suites could finish near the other CI gates and
then prioritized investigating the repeated preparation. Baseline run 38000779070
passed all 41 guest assertions in 35m00s. VM start to guest-ready (boot, SSH and
fixture transfer) took 9m21s; the first exercise began approximately 21 minutes
after job start. Independent shards repeat that preparation, so sharding alone
cannot meet the roughly ten-minute target.

QEMU 8.2.2's default `max` uses QARMA5 pointer authentication. Its documentation
describes a faster, non-cryptographic implementation-defined algorithm. Compare
only that algorithm first, keeping the CPU features, image, candidate, CPU count,
memory, installation and all assertions identical. No gain is assumed.

## Acceptance

- ARM-01: Existing invocation/default CPU behavior remains unchanged. An explicit
  ARM TCG option selects `max,pauth=on,pauth-impdef=on,pauth-qarma3=off`;
  real KVM still uses `host` and x86 cannot select the ARM-only option.
- ARM-02: The real CLI exposes an effect-free plan, rejects invalid options and
  still requires a candidate for execution. Public CLI regressions fail first.
- ARM-03: Both CI profiles run the entire suite in independent disposable VMs,
  install/reinstall the same fresh candidate, verify hashes and preserve all
  assertions. Both results are required, with separate evidence. No shard or
  application/security behavior change is introduced by this experiment.
- ARM-04: Reports identify CPU model, QEMU version, host phase durations and guest
  phase timestamps so preparation and tests can be compared separately. Guest
  timestamps are described as guest clock measurements, not host stopwatches.
- ARM-05: Run `scripts/check --all`, workflow lint and the exact candidate's CI
  comparisons before adopting a faster default. Report failures and measured
  timing honestly. Pointer-authentication cryptographic strength and real ARM
  hardware are outside the implementation-defined profile's evidence.

## Execution

1. Public CLI and required A/B workflow regressions, then explicit profile and
   phase evidence; keep default and all existing suites.
2. Full isolated verification, then a separate draft PR. It targets main, because
   `checks.yml` runs only for pull requests to main, and carries CI cache PR #60's
   commits until that merges. It is not merged as is.
3. Compare complete guest evidence and timings. Adoption is a separate decision
   after evidence; no release tag, runner purchase or device activation.

## Outcome (2026-10-09)

Run 38012670359 compared both profiles on one candidate (package SHA256
`3a2ed010...6a8295`), each passing the same 41 assertions in the same order:

| | QARMA5 (`max`) | implementation-defined |
|---|---|---|
| Job | 34m05s | 17m45s |
| `test-vm.py` total | 2001 s | 1014 s |
| Boot to guest-ready | 535 s | 192 s |
| Candidate and dependencies | 390 s | 160 s |
| Reinstall | 134 s | 41 s |
| Sway, PAM and X11 | 292 s | 208 s |
| labwc and Wayfire | 163 s | 85 s |
| Greeter | 323 s | 193 s |
| Host provisioning (not emulated) | 51 s | 87 s |

One sample per profile on separate hosted runners; the host step's 36 s spread is
runner variance and went against the faster profile.

The user approved adoption the same day. ARM-01 to ARM-03 are superseded: an
emulated aarch64 guest always gets `max,pauth=on,pauth-impdef=on,pauth-qarma3=off`,
the `--tcg-pauth` selector and the two-profile matrix are removed, and the job and
its evidence artifact have their original names again. KVM still uses `host` and
emulated x86 `max`. ARM-04 stays: reports keep the CPU model, QEMU version and
phase times. `--print-plan` and `scripts/test-vm-emulation.py` stay as the public
regression for the CPU choice (six cases, one of which refuses the old selector).

Not done here: the remaining 17m45s is 8.3 min before any suite starts and 8.1 min
of suites, about 2.6 min of that in fixed sleeps scaled by 8. Lowering that scale,
running suites in parallel guests and starting the boot before the package build
ends each need their own measurement and decision.
