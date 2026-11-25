# Design system

All tokens live in `src/views/theme.rs`. Views never use raw colours, `Color32::from_rgb`, or literal font sizes.

## Themes

Two palettes, `DARK` and `LIGHT`. Workspace panels are inset cards (`panel`, with a `border` outline) on a `backdrop` that is lighter than the cards in dark mode, like VS Code's panes; `GAP` of backdrop separates them. `canvas` stays the recessed fill inside cards (inputs, headers); `DARK` canvas is true black for OLED. Controls (`surface`, `hover`) lift above the cards. The app follows the OS (`ThemePreference::System`). `theme::sync` runs once per frame; token functions (`fg()`, `danger()`, …) read the active palette, so call them at draw time, never store them in a `const` or `static`.

## Colour roles

| Role | Tokens | Use |
|---|---|---|
| Surfaces | `backdrop`, `canvas`, `panel`, `surface` | window gaps and toolbar, inputs/recessed fills, panel cards, cards/buttons |
| Lines | `border`, `border_strong` | dividers, table separators and the soft hairline of secondary buttons; input outlines (`border_strong`, 3:1 on every surface, tested) |
| Text | `fg`, `fg_muted` | primary, secondary |
| Brand | `accent` | selection, links, keys |
| Status | `success`, `info`, `warning`, `danger`, `violet` | method/status colours, pills, diffs |

Use `tint(color, Tint::Subtle|Normal|Strong)` for washes, not `gamma_multiply`.

## Type scale

`FS_CAPTION` 12, `FS_SMALL` 13, `FS_BODY` 14, `FS_TITLE` 16. Nothing smaller than 12.

## Components

`card`, `card_panel` (workspace panels), `bar_frame` (toolbar), `panel_frame` (bars inside windows), `pill`, `badge` (table cells), `status_dot`, `chip` (filter tab), `segmented`, `list_row` (sidebar), `capsule` (grouped toolbar actions), `sidebar_toggle`, `action_button`, `action_button_sized` (fixed width, hand-drawn), `icon_button`, `checkbox`, `toolbar_menu`, `menu_item`, `menu_width`, `flat`, `muted`, `overline`; `body::kv_table` for key/value lists. Shortcut hints use `cmd()` (⌘ on macOS, Ctrl+ elsewhere). `menu_item` takes an optional hint, drawn right-aligned in `fg_muted` and left out of the spoken label.

Button hierarchy:
- `action_button` is for the primary or destructive action in a view. It uses a strong tint and coloured border.
- Secondary: `icon_button` with a label and `toolbar_menu` are a quiet `surface` fill with a `border` hairline; hover lifts the fill to `hover` and pressed uses an accent wash, so accent stays rare. Inside a `capsule` (a single `surface` pill) they are flat, so grouped toolbar actions are not individually boxed.
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
