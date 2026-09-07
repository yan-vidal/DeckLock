# Default media pack

Place approved original images in `images/` and videos in `videos/`.
These are ordinary runtime files, not embedded Rust bytes. No personal media is
included automatically. Add credits and an explicit redistribution license for
each supplied work before publishing the pack.

A system package installs this directory at `$datadir/decklock/media` (normally
`/usr/share/decklock/media`). A portable package can place `media/` beside the
binary or use `bin/decklock` plus `share/decklock/media`. User-installed packs
can use `$XDG_DATA_HOME/decklock/media` (default `~/.local/share/decklock/media`).
Development builds also read this source directory.

User imports are separate: `$XDG_DATA_HOME/decklock/library/{images,videos}`.
Updating the bundled pack does not overwrite imports or configured pools.
