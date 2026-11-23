# Design system

All tokens live in `src/views/theme.rs`. Views never use raw colours, `Color32::from_rgb`, or literal font sizes.

## Themes

Two palettes, `DARK` and `LIGHT`. `DARK` is true black (`canvas` and `panel` are `#000`) for OLED screens; `border` separates panels, and only raised controls (`surface`, `hover`) lift off black. The app follows the OS (`ThemePreference::System`). `theme::sync` runs once per frame; token functions (`fg()`, `danger()`, …) read the active palette, so call them at draw time, never store them in a `const` or `static`.

## Colour roles

| Role | Tokens | Use |
|---|---|---|
| Surfaces | `canvas`, `panel`, `surface` | table/inputs, panels, cards/buttons |
| Lines | `border`, `outline` | dividers and table separators; resting outline of secondary controls (3:1 on every surface, tested) |
| Text | `fg`, `fg_muted` | primary, secondary |
| Brand | `accent` | selection, links, keys |
| Status | `success`, `info`, `warning`, `danger`, `violet` | method/status colours, pills, diffs |

Use `tint(color, Tint::Subtle|Normal|Strong)` for washes, not `gamma_multiply`.

## Type scale

`FS_CAPTION` 12, `FS_SMALL` 13, `FS_BODY` 14, `FS_TITLE` 16. Nothing smaller than 12.

## Components

`card`, `panel_frame`, `pill`, `badge` (table cells), `status_dot`, `chip` (filter tab), `segmented`, `list_row` (sidebar), `capsule` (grouped toolbar actions), `sidebar_toggle`, `action_button`, `action_button_sized` (fixed width, hand-drawn), `icon_button`, `checkbox`, `toolbar_menu`, `menu_item`, `menu_width`, `flat`, `muted`, `overline`; `body::kv_table` for key/value lists. Shortcut hints use `cmd()` (⌘ on macOS, Ctrl+ elsewhere).

Button hierarchy:
- `action_button` is for the primary or destructive action in a view. It uses a strong tint and coloured border.
- Secondary: `icon_button` with a label and `toolbar_menu` are filled (`surface`) with an `outline` border at rest, so they read as buttons on OLED black. Hover turns the border `accent`.
- Tertiary: icon-only `icon_button` and `flat` stay borderless for dismiss/copy glyphs that should not compete with other actions.
- `chip` is for multi-select filters and `segmented` is for mutually exclusive tabs. Both reserve stable geometry across selected, hover, and pressed states.

## Hover affordances

This is a desktop app, so no pointer (hand) cursor anywhere; keep the default arrow. Show hover with a fill change instead, and with a small cue where a fill doesn't fit:
- Sortable table headers show a `CARET_UP_DOWN` glyph on hover while unsorted; the active column keeps its `▲`/`▼`.
- App icons in the flow table ease up to 1.25× on hover (about 0.12s) rather than snapping. The sidebar does not zoom.

## Accessibility rules

- Text colours must reach 4.5:1 on every surface and on their own tint. `theme.rs` tests enforce this for both palettes. Changing a colour means the tests must still pass.
- Colour is never the only cue. Chips show a check when on; diff lines keep their `+`/`-` prefix; status pills carry the number.
- Filled buttons use `action_button`: `fg` text on a tinted fill plus a coloured border.
- Keep interactive targets at least `HIT` (28) tall.
