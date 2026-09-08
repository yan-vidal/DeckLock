# Default media pack

The default installation currently includes Yan Vidal's original
[`videos/osaka_dotombori.mp4`](videos/osaka_dotombori.mp4). See [credits](CREDITS.md).
Add future approved original photos under `images/` and videos under `videos/`,
then update the credits table. `scripts/package-release.py` includes the entire
pack automatically; no Rust source changes are needed.

The pack consists of ordinary runtime files, not embedded Rust bytes. Native
packages install it at `/usr/share/decklock/media`. An extracted archive uses
`bin/decklock` plus `share/decklock/media`. User packs can also live in
`$XDG_DATA_HOME/decklock/media` (default `~/.local/share/decklock/media`).
Development builds read this source directory.

A new installation falls back to this pack. Explicit backgrounds, selected pools
and legacy user media folders retain priority. Imports remain separate under
`$XDG_DATA_HOME/decklock/library/{images,videos}`. Package updates never overwrite
imports or configuration.
