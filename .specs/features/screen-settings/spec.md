# Lock, login and rest settings

The user wants the settings application to configure everything the CLI can, with
the login configured from the interface too. Settings get three tabs: Lock, Login
and Rest. Login can mirror Lock with a check box (on by default). Rest is one for
both screens and keeps today's behavior. Applying to the login needs administrator
rights; the interface asks for them through polkit, on an explicit click. Decided
with the user on 2026-10-10; the 0.3.6 release waits for this work.

## Today

- Settings have media tabs "Background" and "Rest" and one "Layout and preferences"
  expander. Layout (alignment, arrangement, spacing, padding, keyboard scale, clock,
  avatar, power options, battery percentage) applies to every screen.
- The login has no settings of its own: `sudo decklock setup greeter` copies the
  user's appearance into `/var/lib/decklock-greeter/appearance.toml`, and the
  greeter applies it (`Config::apply_appearance`). A change reaches the login only
  when setup runs again, from a terminal, with sudo.
- Rest: idle delay, a separate pool or "keep the background and only hide the
  interface", the rest clock. It applies to lock and login alike.

## Requirements

- SCR-01: Config gains a `[login]` section: `same_as_lock` (default true), its own
  `background_pool`, `slideshow_seconds` and `layout`. With `same_as_lock` the login
  uses the lock's values, which is today's behavior. Absent from a saved file
  until changed; older files load unchanged. The CLI reads and writes it
  (`decklock config set login.same_as_lock false`, `login.layout.clock_visible`).
- SCR-02: The greeter uses the login values when `same_as_lock` is false; the lock
  never reads them. Theme, language, rest and the keyboard stay shared.
- SCR-03: Setup's appearance export migrates the login pool's media to the shared
  library like the others and carries the login section into the login snapshot.
  Invalid or unreadable login media fail before anything is migrated.
- SCR-04: Settings show three tabs, Lock, Login and Rest. Lock holds its pool,
  interval and layout. Login holds "Same as the lock screen" (on by default) and,
  when off, its own pool, interval and layout. Rest keeps today's controls. The
  preview shows the screen of the visible tab (login in preview mode never
  contacts greetd). Save keeps unrelated options; restore defaults affects only
  the visible tab's layout.
- SCR-05: An "Apply to login" button runs the login setup with administrator
  rights through polkit (`pkexec`), reusing the installed login's compositor,
  output and rotation. DeckLock never sees the password. Without a polkit agent or
  `pkexec`, the button explains and shows the terminal command instead. Packages
  install a polkit action for exactly that command. Setup identifies the desktop
  user under `pkexec` (`PKEXEC_UID`) as it does under sudo (`SUDO_UID`).
- SCR-06: AGENTS.md states the exception: settings may run setup through polkit,
  only on an explicit click, without seeing the password; preview and settings
  still never authenticate for unlock, acquire a lock or run power actions.

## Tests (each fails before its change)

- Config/CLI: defaults, round trip, `same_as_lock` resolution, unknown keys
  refused, a saved file without `[login]` unchanged by a save.
- Greeter: with an appearance snapshot whose login differs, the greeter view uses
  the login pool and layout; the lock view does not.
- Setup export (real CLI, temporary HOME): the login pool is migrated and carried;
  unreadable login media fail before migration.
- GTK settings: three tabs; mirror check box hides/shows the login controls; save
  writes only what changed; restore on the visible tab; the preview follows the tab.
- Apply to login: a fake `pkexec` in a private PATH records the command; missing
  `pkexec` and a refused authorization report without changing anything. Never the
  real polkit, sudo or greetd.

## Limits

- The real polkit dialog, a real agent and a real `/etc/greetd` write are outside
  the gate; the packaged VM gate already runs the login setup as root.
- Without a polkit agent (common on Hyprland, Sway, labwc, Wayfire) the button
  falls back to the terminal command; DeckLock does not ship an agent.
