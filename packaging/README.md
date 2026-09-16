# Release packages

Version **0.1** is published as GitHub tag `v0.1-r2`; Cargo retains the required
three-component SemVer `0.1.0` internally.

Run `scripts/cargo-local build --release --locked`, then
`python3 scripts/package-release.py --target arch` (or `--target fedora`). Build
the emitted recipe inside `dist/`: `makepkg` on Arch, `rpmbuild` on Fedora. Run
each target **inside that distribution**, because the binary is shipped as built
and its linked baseline is that distribution's. Packaging never installs on the
host, writes user settings, configures the system's own PAM policy, or enables
idle/lock services. The desktop launcher opens settings only.

Outputs:

- `decklock-0.1-linux-x86_64.tar.gz`: binary, desktop entry, themes and media.
- `decklock-0.1-2-x86_64.pkg.tar.zst`: native pacman package containing that payload.
- `pam/decklock`: the PAM service the default `pam_service` expects, installed to
  `/etc/pam.d/decklock` and listed in `backup=` so pacman never replaces an edited
  copy. It carries only `auth` and `account` includes: DeckLock authenticates and
  checks the account, and never opens a session. Each distribution needs its own
  (`packaging/<distro>/pam/decklock`); Debian and Ubuntu include `common-auth` and
  `common-account` rather than `system-auth`.
- `PKGBUILD` or `decklock.spec`: recipe with a checksum for the archive. The spec
  verifies that checksum in `%prep`, the way `sha256sums` does for makepkg.
- `SHA256SUMS`: archive hash; append the package hash after makepkg or rpmbuild.

This dynamically linked binary is not portable across distributions, so each
target records its own baseline rather than reusing another's.

`arch/PKGBUILD.in` lists conservative versions of the Arch build stack. Those are
not API requirements: the code needs GTK 4.12 features, while the recipe pins
`gtk4>=4.22` because that is what Arch shipped at build time. Review them when the
build environment changes.

`fedora/decklock.spec.in` declares no library versions at all. rpm derives them
from the binary's ELF headers, which is why only the GStreamer plugins, loaded
through dlopen and therefore invisible to that scan, are declared by hand. That
scan is more precise than a hand-maintained floor: for the 0.2.0 binary it
derived `GLIBC_2.39` from symbol versions.

Fedora 43 ships gtk4 4.20, glib2 2.86, glibc 2.42 and gstreamer 1.26 -- all older
than the Arch stack -- and packages `gtk4-layer-shell` 1.3.0, so nothing is
bundled. The spec disables strip and debuginfo extraction so the packaged bytes
stay the bytes `test-package.py` verified; the RPM's `/usr/bin/decklock` is
byte-identical to the archive payload.

Archive names are per target, and Arch keeps its published spelling
(`decklock-<version>-linux-x86_64`) because its release assets and the README
install examples point at it. Fedora uses `decklock-<version>-fedora-x86_64`, with
the exact build distribution recorded in `BUILD-INFO.json` instead of in the
filename, so the name survives a Fedora release.

The media payload comes from `assets/media`, including credits. Pacman owns it
under `/usr/share/decklock/media`. Imported media and settings stay in the user's
XDG directories. Default pools discover the pack only when explicit/legacy user
backgrounds are absent; upgrades do not override user choices.

Inspect with `pacman -Qip dist/*.pkg.tar.zst` and `pacman -Qlp dist/*.pkg.tar.zst`.
Extract into a staging directory to check the binary and media before uploading.
Publish the archive, package, PKGBUILD and checksums as release assets, not Git
source files. Source media are intentionally versioned so future commits can add
artwork. Include verification limits in release notes.

## Automated builds

Pull requests build candidate packages for every target through
`build-package.yml`, which takes the target and its pinned container image.
Only the Arch package is published: `release.yml` downloads `arch-package`
alone, because no VM gate installs the RPM yet. Publishable
releases are built by `release.yml` from a new version/revision tag after merge.
The workflow runs the required checks, verifies tag metadata and main ancestry,
then uploads the checked assets. Manual commands above are for local inspection;
CI is the normal release path. See [verification policy](../docs/testing.md).
