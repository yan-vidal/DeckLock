# Release packages

Version **0.1** is published as GitHub tag `v0.1`; Cargo retains the required
three-component SemVer `0.1.0` internally.

Run `scripts/cargo-local build --release --locked`, then
`python3 scripts/package-release.py`. On Arch, run `makepkg --nodeps` inside
`dist/`. Packaging never installs on the host, writes user settings, configures
PAM, or enables idle/lock services. The desktop launcher opens settings only.

Outputs:

- `decklock-0.1-linux-x86_64.tar.gz`: binary, desktop entry, themes and media.
- `decklock-0.1-1-x86_64.pkg.tar.zst`: native pacman package containing that payload.
- `PKGBUILD`: recipe with a checksum for the archive.
- `SHA256SUMS`: archive hash; append the package hash after makepkg.

The first release is built on current Arch x86_64. Runtime requirements in
`arch/PKGBUILD.in` are conservative versions of that build stack; review them when
changing build environments. This dynamically linked binary is not advertised as
portable across distributions. Future distribution-specific builds should record
and test their own dependency baseline rather than reuse the Arch filename.

The media payload comes from `assets/media`, including credits. Pacman owns it
under `/usr/share/decklock/media`. Imported media and settings stay in the user's
XDG directories. Default pools discover the pack only when explicit/legacy user
backgrounds are absent; upgrades do not override user choices.

Inspect with `pacman -Qip dist/*.pkg.tar.zst` and `pacman -Qlp dist/*.pkg.tar.zst`.
Extract into a staging directory to check the binary and media before uploading.
Publish the archive, package, PKGBUILD and checksums as release assets, not Git
source files. Source media are intentionally versioned so future commits can add
artwork. Include verification limits in release notes.
