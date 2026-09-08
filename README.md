# DeckLock

**English** · [Português (Brasil)](README.pt-BR.md)

A customizable **Wayland lock screen**, built with Rust and GTK4. External themes,
photo and video backgrounds, an embedded keyboard and native visual settings.
Optional controller support includes devices such as the Steam Deck.

![DeckLock — keyboard, password visibility and power tooltips](docs/assets/demo.gif)

## Install

### Arch Linux · x86_64

Download the **0.1** package from [GitHub Releases](https://github.com/yan-vidal/DeckLock/releases/tag/v0.1-r2),
or use:

```sh
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.1-r2/decklock-0.1-2-x86_64.pkg.tar.zst
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.1-r2/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
sudo pacman -Syu
sudo pacman -U ./decklock-0.1-2-x86_64.pkg.tar.zst
```

The package installs the application, a **DeckLock Settings** launcher and the
included media pack. Pacman resolves the runtime dependencies; no Rust toolchain
is needed. This is a GitHub download, not an AUR or official Arch package.

Open **DeckLock Settings** from your app launcher, or run:

```sh
decklock --settings
```

### Other Linux distributions

DeckLock is not tied to Arch. It needs GTK4, GStreamer, Linux-PAM and
`gtk4-layer-shell` 1.3+, plus a Wayland compositor supporting `ext-session-lock-v1`
for real locking. X11 is not supported.

The 0.1 prebuilt archive targets the current **Arch x86_64** library stack; it is
not a universal Linux binary. Packages for other distributions and architectures
are not yet provided. See [build from source](#development) for another distribution.

## Preview and lock

```sh
decklock --preview                 # Try without locking
decklock --preview --keyboard      # Show the embedded keyboard
decklock --lock                    # Explicitly lock the session
```

With no arguments, `decklock` prints help. Use `decklock --help` or
`decklock config --help` for commands and examples. Preview never authenticates or runs
power actions. Escape hides the keyboard, then closes the preview.

**0.1 is experimental.** Preview/settings have been tested on Hyprland. Isolated
protocol tests cover lock acquisition, monitor changes and termination without
unlocking. Real-session PAM and broader compositor/controller coverage still
need validation before replacing an existing locker.

## Themes and language

Select Classic, Catppuccin Mocha/Latte, Dracula, Nord, Tokyo Night or Gruvbox.
Colors update settings and the open preview immediately, without restarting the
video for palette-only changes. The language selector switches between English
and Brazilian Portuguese without discarding unsaved edits.

![Choose a theme and see the result live — 7 seconds](docs/assets/settings-themes.gif)

The settings background has a subtle woven texture, controlled by CSS. Cards and
controls retain readable surfaces. **Open preview** shows unsaved changes;
**Save** persists your configuration.

## Background library

The left side is your **media library**; the right side is the **selected pool**.
Use the Images/Videos tabs, select an item and click **Add →**. Removing an item
from the pool does not delete its file. The ⓘ tooltip explains selection behavior.
The eye opens one reusable viewer for images and muted videos.

![Browse videos and inspect a media item — 8 seconds](docs/assets/settings-library.gif)

Each lock randomly selects a pool item. A video loops for that session; a photo
starts a crossfading slideshow of the pool's photos, using the configured interval.
**Import media** copies your files to `~/.local/share/decklock/library` (or
`$XDG_DATA_HOME/decklock/library`) without overwriting existing names.

## Included media

The default installation includes this original video by **Yan Vidal**. Click the
thumbnail to open the file. Additional original photos/videos will be added in
future releases.

[![Osaka Dōtonbori — included video](docs/assets/osaka-thumbnail.jpg)](assets/media/videos/osaka_dotombori.mp4)

**Osaka Dōtonbori** · video · 1920×1080 · 10 seconds · loops without sound in DeckLock.
[Credits and distribution notes](assets/media/CREDITS.md).

The pack is installed at `/usr/share/decklock/media`, outside the Rust executable.
On a fresh setup it supplies the default background. Updates preserve imported
files and your selected pools. Existing explicit backgrounds retain priority.
See the [media pack guide](assets/media/README.md) to add future artwork.

## Rest mode

Choose the **Rest** tab to see the idle appearance in the open preview. Set its
inactivity time, independent media pool and photo interval, or keep the normal
background and hide the controls. **Disable idle mode** hides all dependent options
while preserving their values. The **Background** tab returns to the normal preview.

![Preview rest mode and toggle it off/on — 9 seconds](docs/assets/settings-rest.gif)

The clock stays visible by default, including when reusing the normal background.
Themes can change this with `[layout] idle_clock_visible = false`. Rest mode is
DeckLock's own visual inactivity mode; it does not suspend the computer.

## Layout and custom CSS

Expand **Layout & preferences** to change alignment, spacing, padding, keyboard
scale and visibility. **Edit CSS & theme.toml** opens the live theme editor:
CSS controls appearance, while TOML controls supported layout options.

![Open the CSS editor and switch to layout — 11 seconds](docs/assets/settings-editor.gif)

Valid edits appear live. Invalid drafts retain the last valid appearance and
cannot be saved. Saving creates an editable theme copy under your configuration
directory without overwriting the original theme. No recompilation is needed.

Settings are stored at `~/.config/decklock/config.toml`. `--config PATH` selects
another file. GUI `[layout]` values override theme defaults. Existing PAM settings
are preserved when saving. [Example configuration](config.example.toml).

- [Themes and palette credits](themes/README.md)
- [CSS selectors and layout guide](docs/themes.md) (Portuguese)
- [Translation guide](docs/i18n.md) (Portuguese)

## Configure from the terminal

Every configuration field is also available through the binary, without opening
GTK windows. These commands use the same validated, atomic save as the interface:

```sh
decklock                          # Usage and examples
decklock config --help
decklock config path
decklock config show
decklock config get idle_seconds
decklock config set theme_preset catppuccin-mocha
decklock config set locale en-US
decklock config set idle_seconds 120
decklock config set idle_reuse_background true
decklock config set layout.padding 48
decklock config set layout.idle_clock_visible false
decklock config set background_pool '["/path/photo.jpg", "/path/video.mp4"]'
decklock config unset layout       # Inherit the theme layout again
decklock config import ./my-config.toml
```

`set` accepts numbers, booleans, TOML arrays/tables and plain strings. Unknown
fields and invalid values fail without modifying the file. `unset` resets a key
to its schema default; unsetting an optional field restores inheritance.
`--config PATH` selects another configuration file for any command. For example:

```sh
decklock --config ./demo.toml config set idle_enabled false
```

Custom themes still use ordinary `style.css` and `theme.toml` files: edit them with
your terminal editor, then select the directory with `decklock config set theme /path/to/theme`. Use `decklock config unset theme` to return to bundled presets.
CLI commands change the saved file; reopen an existing lock/preview to load it.
The visual editor's live preview follows that editor's unsaved controls.

## Keyboard and power controls

Mouse, physical keyboard and optional controller input use the same password
field. Double-tap Shift to latch Caps Lock; tap again to release. Alt provides
supplementary symbols when the system layout has no AltGr layer. Keyboard layout
uses the first GDK keymap group at startup.

The external sc-controller daemon is optional:

```sh
decklock --preview --controller
# From another terminal or shortcut:
decklock --toggle-keyboard
```

The settings preview never captures a controller. Normal controller preview needs
explicit `--controller` or `--controller-socket`. Existing `deck-osk --toggle`
shortcuts remain compatible. Capture is released when the keyboard closes.

Power buttons call `systemctl suspend`, `hibernate`, `reboot` and `poweroff`.
Permissions and working sleep/hibernate behavior belong to the host system.
Preview buttons only show tooltips. Procedural backgrounds and plugins are not
implemented; media playback uses installed GStreamer codecs.

## Automated checks

Every PR runs the full regression gate and builds an Arch candidate package on
GitHub. Main requires passing checks; public releases are built and verified from
version tags after merge. See [test coverage and limits](docs/testing.md) and
[agent rules](AGENTS.md).

## Development

Requires Rust 1.93+, GTK4 4.12+ development files, GStreamer base/good/GL libraries
and codecs, Linux-PAM, pkg-config and gtk4-layer-shell 1.3+. On Arch:

```sh
sudo pacman -S --needed base-devel rust gtk4 gtk4-layer-shell gstreamer gst-plugins-base gst-plugins-good gst-libav pam
git clone https://github.com/yan-vidal/DeckLock.git
cd DeckLock
cargo build --release --locked
cargo run -- --settings
```

If the distribution does not provide gtk4-layer-shell 1.3+, the local bootstrap
requires Meson, Ninja, a C compiler, Wayland and wayland-protocols:

```sh
scripts/bootstrap-native
scripts/cargo-local build --release --locked
scripts/cargo-local run -- --preview
```

The bootstrap verifies a pinned archive and installs only into `.deps/`.

### Checks

```sh
scripts/cargo-local test --locked
scripts/cargo-local fmt --all -- --check
scripts/cargo-local clippy --locked --all-targets -- -D warnings
scripts/cargo-local run --locked --example settings_check
scripts/cargo-local run --locked --example settings_live_check
scripts/cargo-local run --locked --example preview_check
scripts/cargo-local run --locked --example media_check
python3 scripts/test-controller-shortcut.py
scripts/bootstrap-native --tests
python3 scripts/test-lock-isolated.py
```

GUI tests use temporary configurations. Protocol tests use a separate Wayland
socket and never lock the active desktop. Documentation GIFs show the real GTK
interface; the capture harness moves the pointer before applying each action.

### Build release packages

```sh
python3 scripts/package-release.py
cd dist
makepkg --nodeps
sha256sum decklock-*.pkg.tar.zst >> SHA256SUMS
```

This stages the optimized binary and `assets/media` together. It does not install
or enable a locker on the build machine. The emitted Arch recipe records runtime
requirements for the release build. [Packaging details](packaging/README.md).

The application runs in Rust; Python is used only for development helpers and
the optional legacy theme importer. The old Python app remains in
[Git history](https://github.com/yan-vidal/DeckLock/tree/7459bb1).
[Implementation status](docs/rust-migration.md).
