# Themes

The settings selector includes DeckLock Classic, Catppuccin Mocha, Catppuccin
Latte, Dracula, Nord, Tokyo Night and Gruvbox Dark. These are DeckLock adaptations
of the named palettes, not official ports or an endorsement by their authors.

Selecting a preset immediately updates the settings window. Save persists
`theme_preset` in the user configuration; Open preview uses the unsaved choice.
The lock screen and embedded keyboard use the same palette. Presets preserve the
existing lock geometry and readable white clock/date overlays on media.

```toml
theme_preset = "catppuccin-mocha"
# Other IDs: classic, catppuccin-latte, dracula, nord, tokyo-night, gruvbox
```

Choose **External theme** to load a directory containing `theme.toml` and
`style.css`. The existing `theme = "/path/to/theme"` takes precedence over the
preset. External CSS is loaded after the shared settings shell and can override
`settings_bg`, `settings_surface`, `settings_raised`, `settings_text`,
`settings_muted` and `accent` via GTK `@define-color`. Settings rules are scoped
under `window.settings` / `.settings`; lock hooks remain documented in the default
CSS. Failed live CSS loads keep the last valid appearance and report the error.

Palette data: [presets.toml](presets.toml). Settings geometry: [settings.css](settings.css).
Default lock geometry: [default/style.css](default/style.css).

## Palette references and credits

- [Catppuccin palette](https://catppuccin.com/palette/), Catppuccin contributors.
- [Dracula palette](https://draculatheme.com/contribute), Zeno Rocha and Dracula contributors.
- [Nord palettes](https://www.nordtheme.com/docs/colors-and-palettes/), Nord contributors.
- [Tokyo Night](https://github.com/folke/tokyonight.nvim), folke and contributors.
- [Gruvbox palette](https://github.com/morhetz/gruvbox/blob/master/colors/gruvbox.vim), morhetz and contributors.

The CSS and widget layouts here are DeckLock's own implementation. Some muted,
hover and selection colors are adapted for readable GTK controls.

## Live editing and idle layout

**Edit CSS & theme.toml** applies valid drafts live. Saving settings creates a new
editable theme copy under the configuration directory; bundled/original themes
are never overwritten. Invalid drafts keep the last valid appearance and block Save.

The settings Background/Rest tabs control the open preview without acquiring a
session lock or capturing the controller. `clock_visible` controls the normal
clock; `idle_clock_visible` controls the idle clock independently (default `true`).

The settings background has a subtle woven CSS texture. Remove it in an external
theme with `window.settings { background-image: none; }`, or adjust the two
repeating gradients in [settings.css](settings.css). Cards retain solid surfaces.
