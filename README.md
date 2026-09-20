<p align="center"><img src="assets/icons/decklock.svg" width="112" height="112" alt="DeckLock"></p>

# DeckLock

**English** · [Português (Brasil)](README.pt-BR.md)

A customizable **Wayland lock screen**, built with Rust and GTK4. External themes,
photo and video backgrounds, an embedded keyboard and native visual settings.
Optional controller support includes devices such as the Steam Deck.

![DeckLock — keyboard, password visibility and power tooltips](docs/assets/demo.gif)

## Install

Version **0.3.1** is published for Arch, Fedora 43 and Ubuntu 26.04 on
[GitHub Releases](https://github.com/yan-vidal/DeckLock/releases/tag/v0.3.1). Each
package installs the application, a **DeckLock Settings** launcher, the included
media pack and the PAM service DeckLock uses. The package manager resolves the
runtime dependencies; no Rust toolchain is needed. These are GitHub downloads, not
packages from official repositories.

Locking only works on one of the [desktops that work](#which-desktops-work):
installing on a stock GNOME or KDE Plasma desktop will not let DeckLock lock it.

### Arch Linux · x86_64

```sh
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.3.1/decklock-0.3.1-1-x86_64.pkg.tar.zst
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.3.1/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
sudo pacman -Syu
sudo pacman -U ./decklock-0.3.1-1-x86_64.pkg.tar.zst
```

### Fedora 43 · x86_64

```sh
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.3.1/decklock-0.3.1-1.fc43.x86_64.rpm
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.3.1/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
sudo dnf install ./decklock-0.3.1-1.fc43.x86_64.rpm
```

### Ubuntu 26.04 · x86_64

```sh
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.3.1/decklock_0.3.1-1_amd64.deb
curl -fLO https://github.com/yan-vidal/DeckLock/releases/download/v0.3.1/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
sudo apt install ./decklock_0.3.1-1_amd64.deb
```

Each package is built against its own distribution's libraries. Derivatives that
share those repositories, such as EndeavourOS or Fedora spins, may work but are not
tested. **Ubuntu 24.04 and Debian 13 are not supported:** Ubuntu 24.04 does not
package `gtk4-layer-shell` at all, and Debian 13 packages a version older than the
1.2 DeckLock needs.

Open **DeckLock Settings** from your app launcher, or run:

```sh
decklock --settings
```

### Other Linux distributions

DeckLock is not tied to Arch. It needs GTK4, GStreamer, Linux-PAM and
`gtk4-layer-shell` 1.2+, and one of the [desktops that work](#which-desktops-work).

The packages above are not universal Linux binaries: each targets its own
distribution's library stack, on x86_64 only. For another distribution, see
[build from source](#development).

The package installs `/etc/pam.d/decklock`, which `pam_service` now defaults to.
Installing from the archive or from source does not create it, and DeckLock refuses
to lock rather than trap you behind a screen it cannot authenticate: create the file
with `auth include` and `account include` lines for your distribution's stack (see
`packaging/<distro>/pam/decklock`), or point `pam_service` at a service you already
have.


## Procedural media

Open **Background → Library → Procedurals**. Select **Starfield**, **Particles**,
**Lissajous curves**, **Matrix rain**, **Doom fire**, **Aurora**, **Flow field** or
**Ridgeline** and add it to the pool, just like a photo or video. A procedural replaces the background; it is not an overlay. If selected
for a lock, it keeps playing throughout that session. Photo selections still cycle
only among photos. Rest has its own pool; **Reuse background** keeps the normal media.

Use the **eye** to view an item in the reusable media viewer, and the **gear** to
adjust that item's colors, speed, particle count, seed and FPS (1–30). **Apply**
updates the draft and open previews; **Save** in settings writes the configuration.
Closing the item editor without applying discards its edits. Settings belong to the
item and are shared by every pool using it; removing it from a pool keeps its settings.

```sh
decklock config set background_pool '["procedural:starfield"]'
decklock config set procedurals.starfield.color '#b4befe'
decklock config set procedurals.starfield.speed 1.0
decklock config set idle_pool '["procedural:lissajous"]'
decklock config set idle_reuse_background false
decklock --preview
```

The IDs are `procedural:starfield`, `procedural:particles`,
`procedural:lissajous`, `procedural:matrix`, `procedural:doom-fire`,
`procedural:aurora`, `procedural:flow-field` and `procedural:ridgeline`.

Density means particle count, matrix columns, doom-fire flame height (120 keeps
the top dark, 300 fills the frame), aurora bands, flow-field streamlines or
ridgeline rows, depending on the item.

Per-item TOML tables live under `[procedurals.starfield]` (and the other IDs).
`config unset procedurals` restores default parameters. No third-party code runs.
Rendering uses an opaque raster texture capped at 640 pixels on the longest side,
scaled to the window. Hidden widgets stop requesting frames. Battery savings are
not yet measured. See [example configuration](config.example.toml).


## Preview and lock

```sh
decklock --preview                 # Try without locking
decklock --preview --keyboard      # Show the embedded keyboard
decklock --greeter                 # Native greetd login or standalone greeter preview
decklock --lock                    # Explicitly lock the session
```

With no arguments, `decklock` prints help. Use `decklock --help` or
`decklock config --help` for commands and examples. Preview never authenticates or runs
power actions. Escape hides the keyboard, then closes the preview.

**0.3.1 is experimental.** Preview/settings have been tested on Hyprland. Isolated
protocol tests cover lock acquisition, monitor changes and termination without
unlocking, and disposable virtual machines exercise each packaged locker against
real Sway and real Linux-PAM on Arch, Fedora and Ubuntu, including denial, unlock
and account lockout. GPU
paths, physical controllers and power actions, other compositors and other PAM
policies still need validation before replacing an existing locker.

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

## Login and screen lock automation (`decklock setup`)

DeckLock can serve as your unified login greeter (`greetd`), user-switching screen, and compositor screen locker (`hypridle`), keeping the exact same theme and on-screen keyboard everywhere.

Inspect your system's current integration:

```sh
decklock setup status
```

Configure `greetd` to launch DeckLock greeter inside the `cage` kiosk compositor (virtual keyboard enabled by default):

```sh
sudo decklock setup greeter
```

Configure `hypridle` to lock using DeckLock:

```sh
decklock setup lock
```

Reference configuration templates are installed under `/usr/share/decklock/setup/` (`greetd.toml`, `hypridle.conf`, `decklock.service`). Use `--dry-run` to preview file changes without writing to disk.

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

Power and session buttons support fast user switching (`switch_user_command`, `dm-tool`, `gdmflexiserver`, or `loginctl`) as well as `systemctl suspend`, `hibernate`, `reboot` and `poweroff`.
Permissions and working sleep/hibernate behavior belong to the host system.
Preview buttons only show tooltips. Plugins are not implemented; media playback
uses installed GStreamer codecs.

## Help and roadmap

F1 or the help icon opens an offline guide that follows the settings language.
Read [General](docs/guide/en-US/general.md) and [Advanced](docs/guide/en-US/advanced.md).
Ordinary windows have close controls; the title-bar preference is also available through `decklock config set window_decorations false`.

See the [roadmap](docs/ROADMAP.md), [changelog](CHANGELOG.md) and [release procedure](docs/releases.md).

## Compatibility
<a id="which-desktops-work"></a>

DeckLock targets modern Linux desktop sessions. Because session locking interacts directly with display protocols and PAM authentication, compatibility depends on your compositor, distribution, and architecture:

### Compositors & Display Servers

| Environment | Status | Security Guarantee | Validation |
|---|---|---|---|
| **Sway** | ✅ Supported | **Full** (`ext-session-lock-v1`) | Automated disposable VM (Arch, Fedora 43, Ubuntu 26.04) with real PAM |
| **Hyprland, niri, river, Wayfire, Labwc, COSMIC** | ✅ Supported | **Full** (`ext-session-lock-v1`) | Protocol compliance; same Wayland backend |
| **X11 sessions** | ⚠️ Experimental (`--features x11`) | **Reduced** (keystrokes not isolated by protocol; screen unlocks if process dies) | Isolated gate (`Xvfb` + `xfwm4` + intruder probe); accelerated video via GLX/EGL |
| **GNOME & KDE Plasma** | ❌ Not supported | N/A | Refuses to lock safely (both draw their own internal lock screen) |

### Distributions & Packaging

| Distribution | Prebuilt Package | PAM Service Included | VM Automated Test | Notes |
|---|---|---|---|---|
| **Arch Linux** | ✅ `.pkg.tar.zst` | ✅ `/etc/pam.d/decklock` | ✅ Real PAM + faillock | Primary tier |
| **Fedora 43+** | ✅ `.rpm` | ✅ `/etc/pam.d/decklock` | ✅ Real PAM | Stock install requires `authselect` to enable faillock counting |
| **Ubuntu 26.04+** | ✅ `.deb` | ✅ `/etc/pam.d/decklock` | ✅ Real PAM | Packages `gtk4-layer-shell >= 1.3` |
| **Ubuntu 24.04** | ❌ No | — | — | Missing `gtk4-layer-shell` in distribution repositories |
| **Debian 13** | ❌ Blocked | — | — | Ships `gtk4-layer-shell 1.0.4` (below 1.2 requirement) |
| **Other Linux** | ⚙️ Build from source | Manual setup | — | Requires GTK 4.22+, GStreamer 1.28+, Linux-PAM, `gtk4-layer-shell >= 1.2` |
| **BSD (FreeBSD / OpenBSD)** | ❌ Out of scope | — | — | Different auth/power stacks (BSD Auth, OpenPAM without faillock, no systemd) |

### Architectures

| Architecture | Status | Package Availability |
|---|---|---|
| **x86_64** (AMD64) | ✅ Supported | Published packages (`.pkg.tar.zst`, `.rpm`, `.deb`, `.tar.gz`) |
| **aarch64** (ARM64) | 🚧 Supported | Source build and release packaging recipes ready |

**What about X11?** An X11 backend is included in candidate release packages (`--features x11`). X11 gives a weaker lock than Wayland, and the lock screen says so while it is up: any other program in the session can read what you type, and if DeckLock stops, the screen unlocks. Prefer Wayland where you have it; DeckLock never uses X11 in a session that has `ext-session-lock-v1`. Video backgrounds take the accelerated GL path on X11 when built with this feature.

**Why not GNOME or KDE Plasma?** Both draw their own lock screen inside the desktop and do not let another program replace it, so there is nothing for DeckLock to connect to. On them DeckLock refuses to lock and says so, rather than locking in a way that would not be safe. Use the desktop's own lock screen there. Their X11 sessions are not a way around this: GNOME has removed its X11 session and Plasma is dropping its own.

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
