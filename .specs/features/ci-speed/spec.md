# Faster required CI

Authorized 2026-10-09. A measured PR run spent 745 seconds installing Ubuntu
native test dependencies and 271 seconds running the isolated suite. Packages
then waited for that entire job before starting. Improve preparation and the
critical path without reducing any required test or package-install evidence.

## Acceptance criteria

- CI-01: Packages start alongside isolated tests. Each VM waits for its own
  package. The existing final required check still requires all ten jobs, and
  any failed, skipped or cancelled job prevents merge and release.
- CI-02: Native tests run in a dependency-only Ubuntu 24.04 image with digest-pinned
  bases, Rust 1.93.0, every existing native dependency and the pinned mock server.
  Docker layers are cached; apt and native preparation refresh each UTC week.
  No project binary, credential, desktop socket or controller device enters the
  image. Every invocation builds/tests the caller's exact checkout as a nonroot
  user. Image build failures fail the job; a cold cache builds normally.
- CI-03: Cargo caches contain downloaded registry/git dependencies only, separated
  by target, architecture, toolchain/build identity and Cargo.lock. No cached
  target directory or product executable substitutes for current-commit tests.
- CI-04: VM caches contain only the selected pristine image, keyed by its pinned
  SHA256. The VM runner still verifies its hash, creates disposable overlays and
  installs the exact candidate and its dependencies inside the running guest.
  Prepared disks, keys, credentials and logs are excluded.
- CI-05: New workflow contracts and public cache-metadata CLI regressions join
  scripts/check. Existing required names, all assertions, artifact hash checks,
  release provenance and physical-device UAT limits stay intact.

## Execution plan and verification

1. Dependency image and concurrent package scheduling: touch checks.yml,
   .github/ci/native-tests.Dockerfile, its entrypoint, scripts/test-ci-workflow.py,
   scripts/check. Verify regression first, Docker image build, full isolated suite
   and workflow lint. Commit independently.
2. Download caches: touch local cache-cargo/cache-vm composite actions,
   build-package.yml, VM metadata CLI and its public-boundary tests. Verify cold
   and invalid metadata inputs and cache scope/path contracts. Commit independently.
3. Document preparation/refresh costs and coverage, run full local suite, open a
   separate PR and validate all required CI jobs on the exact head. Record observed
   cold/warm timings and failures rather than predicting measured improvement.

No runner provisioning, release tag, real-session restart or Deck UAT is part of
this change. The controller/greeter candidate in PR #59 remains separate.
