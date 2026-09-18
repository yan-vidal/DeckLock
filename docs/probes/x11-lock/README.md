# X11 lock probe (disposable)

Evidence for the X11 backend design in #15/#16. This is not product code: it
never authenticates, and nothing here is built, tested or packaged with DeckLock.

`lock` is a minimal GTK4 window turned into an X11 lock surface. `intruder` is an
ordinary, unprivileged client in the same X session that tries to take the
keyboard, read keystrokes, cover the lock and type into it.

```sh
docs/probes/x11-lock/run.sh none         # no window manager
docs/probes/x11-lock/run.sh xfwm4        # Xfce's window manager, compositor off
docs/probes/x11-lock/run.sh xfwm4-comp   # compositor on (intruder cm confirms it)
docs/probes/x11-lock/run.sh xfwm4 PROBE_FOCUS=keep   # reproduce a failure
```

Each run uses a private Xvfb and D-Bus session with no inherited desktop
variables, and writes logs and screenshots to `target/x11-probe/`. It needs
`xvfb-run`, `dbus-run-session`, `xwd`, ImageMagick and, for the managed cases,
`xfwm4`.

## What works

1. **Override-redirect on a GTK4 window.** GTK4 has no API for it, but the X
   window exists after `realize()` and before it is mapped, so setting
   `override_redirect` through Xlib on GDK's own connection takes effect when GTK
   maps it. The window manager does not reparent it, it covers the monitor, and
   GTK renders at 60 fps, with a compositing manager too.
2. **Grabs on GDK's connection.** Keyboard and pointer grabs succeed on the first
   attempt; another client gets `AlreadyGrabbed`, and GTK keeps receiving keys.
   Grabs from a separate connection were not tried: the grabbing connection
   receives the events, and GTK reads only its own.
3. **Popovers.** Opening and closing an autohide GTK popover does not drop the grabs.

## What the naive design gets wrong

| Mechanism | Naive choice | Observed | What passed |
| --- | --- | --- | --- |
| Keyboard grab | `owner_events=False` | GTK receives no key at all | `owner_events=True` |
| Staying on top | re-raise on `VisibilityNotify` | never reports obscured while the server has the Composite extension, even to `xev`; it does with `-extension Composite`, which Xorg enables by default | `SubstructureNotify` on root (shareable) and re-raise |
| Re-raise filter | any other window at the top of the stack counts as covering | in the run that opened a popover: 9,096 raises and 9 fps (which of the two exclusions ended it was not isolated) | ignore unmapped windows and windows GDK owns: 7 to 12 raises |
| Focus | leave X focus alone | xfwm4 moves X focus when windows are mapped; the grab holds but GTK stops receiving keys | select `FocusChangeMask` ourselves (no core `FocusOut` reached us without it) and re-assert focus |

With those four, the full sequence passed with no window manager, xfwm4 and xfwm4
with its compositor: grab refusal for the intruder, a wrong password, a managed
and an override-redirect window raised every 500 ms, a popover, the correct
password and release. The lock was the topmost window in 19 or 20 of 20 samples
taken during the raise war: a client that keeps raising itself is briefly visible
before the lock reacts.

## What X11 cannot provide, measured

- An unprivileged client reading XI2 raw key events captured `h u n t e r Return`
  while the lock held both grabs, in every environment.
- Killing the lock releases the grabs at once: another client grabs successfully.

So on X11 `Guarantees { survives_process_exit: false, isolates_input: false }`,
and the lock-time warning required by #17 states a measured fact.

## Not covered

Real Xorg (DPMS, VT switching, physical devices), several monitors and XRandR
hotplug, window managers other than xfwm4, DeckLock's real lock screen, on-screen
keyboard and accelerated video (#18). Xvfb has one screen and synthetic input.
