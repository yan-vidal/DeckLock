# Using DeckLock

DeckLock is an experimental, customizable Wayland screen locker. It includes a virtual keyboard, external themes and a media library. Version 0.2 adds procedural backgrounds and an offline guide.

## Start safely with preview

Open settings from the application menu or run:

```sh
decklock --settings
```

Open preview to try your changes. Preview does not lock your session, authenticate a password or run power actions. Escape closes the lock-screen preview. Closing it never unlocks a real session.

Use F1 or the question-mark button for this guide. General covers everyday use; Advanced explains the implementation. The guide follows the language selected in settings and works without an internet connection.

## Save and window controls

Settings preview changes before you save them. Save writes the configuration. Closing settings discards unsaved configuration changes and closes its preview.

All ordinary DeckLock windows provide a close button by default. Under Layout, disable window title bars if you prefer compositor shortcuts. Help remains available with F1. This preference does not add window controls to an actual locked screen.

The procedural editor previews valid changes immediately. Apply accepts its draft into settings; Save persists it. Closing that editor without Apply restores its original parameters. The CSS/layout editor keeps its draft for the main Save button; closing the editor alone does not save to disk.

## Library and selected pool

The library contains available images, videos and built-in procedurals. The pool on the right contains the items eligible for the next lock. Add selected media with the arrow; remove it from the pool without deleting the source file. Import copies your files into the user library.

The eye opens one reusable media viewer. Videos play without audio. Procedurals have a gear button in the library and viewer. Their thumbnails update as you edit them.

A lock chooses an initial item from its pool. If it is an image, only the pool's images participate in the slideshow at the configured interval. A selected video loops for that lock. A selected procedural keeps animating for that lock. An explicitly empty pool shows the fallback background.

## Procedurals and diagnostics

Available effects are Starfield, Particles, Lissajous curves, Matrix rain, Doom fire, Aurora, Flow field and Ridgeline. Speed defaults to 1; density, colors, seed and frame-rate controls depend on the effect. A procedural replaces the background.

The viewer reports generated FPS, drawing CPU, total settings-process CPU and process memory. Process totals include GTK and other settings work; they are not the exclusive cost of the selected effect. Texture size is a pixel-buffer estimate, not total GPU memory. Device performance and battery use can differ from preview measurements.

## Rest mode

Rest is DeckLock's own inactivity mode after the locker has started. It does not schedule the system to launch DeckLock. Configure your desktop or idle daemon separately to start the locker.

The Rest tab previews this state. Choose its own pool and slideshow interval, keep the normal background while hiding controls, or disable rest entirely. The rest clock is configurable. Activity restores the normal interface.

## Attempts and account lockout

Linux systems usually count authentication failures and lock the account for a while. That is PAM policy, not DeckLock's, and it applies to console logins just the same.

When PAM reports that the account is locked, the screen shows that state and, if PAM said how long it lasts, a countdown in minutes and seconds. The countdown is what PAM reported, rounded up: the password field keeps working the whole time and you may try again whenever you want. If your configuration declares the failure threshold explicitly, the screen also reports how many attempts remain.

None of this is enforced by DeckLock, which only repeats what the system said. When information is missing, the screen stays silent instead of estimating a number. The notices use the `#status.warning` and `#status.locked` selectors, which your theme can style.

## Keyboard and power

Use the physical or virtual keyboard. Shift changes case, double Shift latches Caps Lock, and Alt exposes alternate characters. The eye beside the password toggles its visibility. Optional sc-controller integration uses its external daemon and the embedded keyboard.

Suspend, hibernate, restart and shutdown delegate to systemctl. They depend on your system's support, permissions and configuration; DeckLock does not configure hibernation for you. These actions are disabled in preview.

## Themes, media and command line

Choose a built-in palette or an external theme. A theme combines GTK CSS and declarative TOML layout. The settings editor validates changes before saving. Restore returns the corresponding controls or theme draft to built-in defaults.

The bundled Osaka video is supplied by the author. Imported media live under XDG_DATA_HOME/decklock/library, normally ~/.local/share/decklock/library. Legacy normal and rest folders are under XDG_CONFIG_HOME/midias/bloqueio and midias/ocioso, with fotos and videos subfolders. Explicit configuration and pools take priority over defaults.

```sh
decklock --help
decklock config path
decklock config show
decklock config set window_decorations false
decklock config set procedurals.matrix.speed 1.0
decklock config unset window_decorations
```

Actual lock mode requires a compositor supporting the Wayland session-lock protocol. Test your recovery procedure before relying on this experimental locker for a daily session.
