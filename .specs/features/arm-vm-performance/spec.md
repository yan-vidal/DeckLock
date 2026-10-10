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
