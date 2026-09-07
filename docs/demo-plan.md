# README demo recording

The user supplied `osaka_dotombori.mp4`, stored outside Git in the user's media
folder. `examples/record_demo.rs` drives the actual Rust preview in English with
fictitious text; it never acquires a session lock or authenticates.

Suggested 20–25 second sequence:

1. Establish the lock screen with the looping video background.
2. Open the embedded keyboard using its icon.
3. Type example characters with the virtual keyboard.
4. Double-tap Shift; hold briefly on the English Caps Lock notice.
5. Type uppercase characters, then release the Shift latch.
6. Hide the keyboard and reveal the example text using the eye icon.
7. Hide the example text again.
8. Hover Suspend, Hibernate, Restart and Shut down, pausing for each English tooltip.
9. Return to the opening composition for a clean loop.

Export a compact GIF for the README and optionally a higher-quality video.
Preview power buttons support hover but do not execute system commands.

Reproduce the capture on Hyprland with `grim` and an existing `ydotool` daemon:

```sh
scripts/cargo-local build --example record_demo
python3 scripts/record-demo.py /path/to/background.mp4
```

This opens a fullscreen preview and moves/clicks the pointer for about 30 seconds.
The capture directory printed at completion contains PNG frames with the cursor,
plus a preview log. Frame modification times preserve the actual capture timing.
The helper uses real pointer clicks for keyboard/eye controls; power actions are
preview-only and are hovered without executing system commands.
