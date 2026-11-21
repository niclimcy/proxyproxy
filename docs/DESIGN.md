# Design system

All tokens live in `src/views/theme.rs`. Views never use raw colours, `Color32::from_rgb`, or literal font sizes.

## Themes

Two palettes, `DARK` and `LIGHT`. The app follows the OS (`ThemePreference::System`). `theme::sync` runs once per frame; token functions (`fg()`, `danger()`, …) read the active palette, so call them at draw time, never store them in a `const` or `static`.

## Colour roles

| Role | Tokens | Use |
|---|---|---|
| Surfaces | `canvas`, `panel`, `surface` | table/inputs, panels, cards/buttons |
| Lines | `border` | dividers, table separators, control outlines |
| Text | `fg`, `fg_muted` | primary, secondary |
| Brand | `accent` | selection, links, keys |
| Status | `success`, `info`, `warning`, `danger`, `violet` | method/status colours, pills, diffs |

Use `tint(color, Tint::Subtle|Normal|Strong)` for washes, not `gamma_multiply`.

## Type scale

`FS_CAPTION` 12, `FS_SMALL` 13, `FS_BODY` 14, `FS_TITLE` 16. Nothing smaller than 12.

## Components

`card`, `panel_frame`, `pill`, `badge` (table cells), `status_dot`, `chip` (filter tab), `segmented`, `list_row` (sidebar), `capsule` (grouped toolbar actions), `sidebar_toggle`, `action_button`, `icon_button`, `toolbar_menu`, `menu_item`, `menu_width`, `flat`, `muted`, `overline`; `body::kv_table` for key/value lists. Shortcut hints use `cmd()` (⌘ on macOS, Ctrl+ elsewhere).

Button hierarchy:
- `action_button` is for the primary or destructive action in a view. It uses a strong tint and coloured border.
- `icon_button` and `flat` are for low-emphasis actions that should not compete with primary actions.
- `chip` is for multi-select filters and `segmented` is for mutually exclusive tabs. Both reserve stable geometry across selected, hover, and pressed states.

## Accessibility rules

- Text colours must reach 4.5:1 on every surface and on their own tint. `theme.rs` tests enforce this for both palettes. Changing a colour means the tests must still pass.
- Colour is never the only cue. Chips show a check when on; diff lines keep their `+`/`-` prefix; status pills carry the number.
- Filled buttons use `action_button`: `fg` text on a tinted fill plus a coloured border.
- Keep interactive targets at least `HIT` (28) tall.
