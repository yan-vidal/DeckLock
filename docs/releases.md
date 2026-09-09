# Release procedure

The 0.2.0 release is being prepared in a PR. This document does not authorize publication.

1. Define the scope and review CHANGELOG.md. Label PRs enhancement, bug, documentation, maintenance or dependencies; unmatched PRs remain included in Other changes. Review user-facing notes rather than treating commit messages as documentation.
2. Update Cargo.toml/Cargo.lock to the full X.Y.Z version and reset Arch pkgrel to 1 for an application release. A packaging-only rebuild increments pkgrel without changing Cargo; its tag is vX.Y.Z-rN. Legacy v0.1/v0.1-r2 releases retain their existing names and assets.
3. Run scripts/check --all and require both GitHub checks. Package verification includes the real packaged CLI, guide payload, full version names, tag rejection and changelog extraction. PR artifacts are candidates, not published releases.
4. Record manual results for installing/upgrading the candidate package, preserving an existing configuration, keyboard/controller behavior and normal/rest media on the target device. Keep real lock/PAM testing recoverable and explicitly authorized. Record unresolved limits; never describe automated preview tests as device validation.
5. Once the release is approved, replace Unreleased in its changelog heading with the actual release date. Preview the summary with python3 scripts/release-notes.py; validate publication readiness with --require-date. Update README installation examples to the soon-to-be-published full version as part of the release PR, not while that download does not exist.
6. Merge through the protected PR flow. Confirm main checks and tag the reviewed commit with vX.Y.Z (or the packaging-revision tag). Never move a published tag or replace an asset.
7. The tag workflow reruns tests/builds, verifies ancestry/version/checksums and the dated changelog, then publishes an experimental prerelease. The reviewed summary precedes GitHub's generated PR references. Confirm the assets/downloads and notes after publication.

There is no automatic release on PR creation or merge. SHA256 checksums detect changed bytes; they are not signatures. Current CI records source and linked-library provenance, but does not claim bit-for-bit reproducibility or a security audit.
