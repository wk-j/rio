# CR-016: Quick Terminal

**Status:** Implemented
**Date:** 2026-03-01
**Author:** wk

## Summary

Implement a quick terminal overlay — a persistent PTY panel that floats over
existing panes with a single keybinding. Inspired by iTerm2's Hotkey Window and
Guake. Toggling the same key shows/hides the panel without destroying the shell
session. The panel can be anchored to any edge of the window (top, bottom, left,
right) or centered, leaving main panes fully visible around it. Appearance,
position, and geometry are fully configurable.

## Motivation

1. **Fast context switching** — reach a scratch shell instantly without leaving the
   current workflow or creating a new tab.
2. **Session persistence** — the PTY stays alive when hidden; processes keep running.
3. **CWD inheritance** — opens in the working directory of the active pane so
   clipboard-less file path reuse is immediate.
4. **Resizable** — height is adjustable live with the existing divider resize keys.
5. **2D layer feel** — main pane content remains visible above the panel, making
   the quick terminal feel like a floating layer rather than a modal takeover.

## User Flow

```
1. User presses ToggleQuickTerminal binding (e.g. Ctrl+`)
2. Floating panel appears at the configured position (default: bottom, 40%)
3. Main pane content stays visible around the panel
4. User runs commands in the panel
5. Press binding again → panel hides, focus returns to previous pane
6. Press binding again → panel re-appears with the same shell, same history
7. User types `exit` → shell exits, panel is destroyed; pane regains focus
8. Window resize → panel auto-dismisses (hides, not destroyed)
9. Switching tab / creating split → panel auto-dismisses
```

### Visual Examples

#### Bottom (default)
```
+--------------------------------------------------+
| $ cargo build                                     |   ← main pane (visible)
|    Compiling rio v0.2.0                          |
|    ...                                           |
+┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄+
|╭────────────────────────────────────────────────╮|   ← floating panel
|| $ █                                            ||
||                                                ||
|╰────────────────────────────────────────────────╯|
+--------------------------------------------------+
```

#### Top (dropdown, Guake/Yakuake style)
```
+--------------------------------------------------+
|╭────────────────────────────────────────────────╮|   ← floating panel
|| $ █                                            ||
||                                                ||
|╰────────────────────────────────────────────────╯|
+┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄+
| $ cargo build                                     |   ← main pane (visible)
|    Compiling rio v0.2.0                          |
|    ...                                           |
+--------------------------------------------------+
```

#### Left / Right
```
+------------------------+-------------------------+
|╭──────────────────────╮| $ cargo build            |
|| $ █                  ||    Compiling rio v0.2.0  |
||                      ||    ...                   |
||                      ||                          |
||                      ||                          |
|╰──────────────────────╯|                          |
+------------------------+-------------------------+
         panel ↑                main pane ↑
```

#### Center
```
+--------------------------------------------------+
| $ cargo build                                     |
|  ╭──────────────────────────────────────────╮     |
|  │ $ █                                      │     |
|  │                                          │     |
|  ╰──────────────────────────────────────────╯     |
|    ...                                           |
+--------------------------------------------------+
```

Corners that touch a window edge are sharp; interior corners are rounded.
The panel has a configurable border and drop shadow. Main pane content is
rendered normally around it.

## Architecture

### State

```rust
/// State for the quick terminal overlay pane.
/// The quick terminal is rendered as an overlay on top of main panes at the
/// configured position — main pane dimensions are never modified.
pub struct QuickTerminalState<T: EventListener> {
    /// The quick terminal's context item (outside the normal split tree)
    pub item: ContextGridItem<T>,
    /// Whether the quick terminal is currently visible
    pub visible: bool,
    /// The route_id of the pane that was focused before the QT was shown
    pub saved_focus: usize,
}
```

`QuickTerminalState` lives as `Option<QuickTerminalState<T>>` on `ContextGrid` —
outside the normal `inner: HashMap` of split panes. `None` means the QT has never
been opened on this tab; `Some` means the PTY exists (visible or not).

### Lifecycle

```
ToggleQuickTerminal action
        │
        ▼
ContextManager::toggle_quick_terminal(rich_text_id)
        │
        ├─► quick_terminal is None?
        │       │
        │      Yes ──► spawn PTY via create_context()
        │               inherit CWD from foreground process
        │               call grid.open_quick_terminal(context)
        │
        └─► quick_terminal is Some?
                │
                ├─► visible = true  ──► hide (visible=false, restore saved_focus)
                └─► visible = false ──► show (visible=true, save focus, update dims)
```

### Panel Geometry

The panel position is set via `QuickTerminalConfig::position` (default: `Bottom`).
Two separate fields control sizing:
- `height` (0.0–1.0 fraction, default 0.4) — fraction of window height
- `width` (0.0–1.0 fraction, default 0.4) — fraction of window width

All geometry is computed by `ContextGrid::qt_panel_geometry()`:

| Position | Panel width | Panel height | Anchor |
|----------|-------------|--------------|--------|
| `bottom` | full width − margin | `height` × window height | Bottom edge |
| `top` | full width − margin | `height` × window height | Top edge |
| `left` | `width` × window width | full height − margins | Left edge |
| `right` | `width` × window width | full height − margins | Right edge |
| `center` | `width` × window width | `height` × window height | Centered |

Minimum panel size is 60 scaled pixels on the relevant axis.
Main pane dimensions are never modified — the panel is a pure visual overlay.

### Border Radius Per Position

`QuickTerminalPosition::border_radius(r)` returns the corner radii array,
rounding only the corners that do not touch a window edge:

| Position | top-left | top-right | bottom-right | bottom-left |
|----------|----------|-----------|--------------|-------------|
| `bottom` | r | r | 0 | 0 |
| `top` | 0 | 0 | r | r |
| `left` | 0 | r | r | 0 |
| `right` | r | 0 | 0 | r |
| `center` | r | r | r | r |

### Resize

For horizontal positions (top/bottom/center), `MoveDividerUp` / `MoveDividerDown`
are intercepted and forwarded to `resize_quick_terminal(±20.0)` which adjusts the
panel height. For vertical positions (left/right), `MoveDividerLeft` /
`MoveDividerRight` adjust the panel width instead.

The resized dimension is clamped between 10%–80% of the relevant window axis
(or 60px minimum). After resizing, the panel is reanchored to its configured
edge (or recentered for the center position).

### Rendering

Main panes render normally at all times — the QT is a 2D layer on top, not a
replacement. The object list for a frame is:

```
[main pane quads + RichTexts]   ← rendered first, visible above panel
[QT background Quad]            ← rounded top corners, border, drop shadow
[QT RichText]                   ← terminal content rendered on top
```

The QT `RichText` is always rendered with `TerminalDamage::Full` and
`bg_opacity_override = Some(config.opacity)` so default-background cells use the
configured opacity.

Panel background quad uses position-dependent border radii — corners touching
a window edge are sharp, interior corners are rounded (see table above).

### Dismissal Sites

The QT is auto-dismissed (hidden, PTY preserved) by:

| Trigger | Location |
|---------|----------|
| Window resize | `context/grid.rs` `resize()` |
| Switch tab | `context/mod.rs` `select_tab()` |
| New tab | `context/mod.rs` `add_context()` |
| New split | `context/mod.rs` `split()` |
| Close split | `context/mod.rs` `remove_current_grid()` |
| Close context | `context/mod.rs` `close_current_context()` |
| Next tab | `context/mod.rs` `switch_to_next()` |
| Prev tab | `context/mod.rs` `switch_to_prev()` |

When the QT shell exits (`exit` / EOF), the overlay is **destroyed** (not just
hidden) and focus returns to `saved_focus`.

### Tab Bar

When the QT is visible, the tab bar renders no tab as active (`!qt_visible &&
i == current`), reflecting that the overlay is transient and does not belong to
any specific split layout.

## Configuration

### Keybinding

```toml
[bindings]
keys = [
    { key = "`", mods = "Control", action = "ToggleQuickTerminal" },
]
```

### `[quick-terminal]` section

```toml
[quick-terminal]
# Panel position: "top", "bottom", "left", "right", or "center". Default: "bottom"
position = "bottom"

# Panel height as a fraction of window height (0.1–0.9). Default: 0.4
# Used by top, bottom, and center positions.
height = 0.4

# Panel width as a fraction of window width (0.1–0.9). Default: 0.4
# Used by left, right, and center positions.
# For top/bottom the panel always spans the full window width.
width = 0.4

# Background opacity (0.0 = transparent, 1.0 = opaque). Default: 1.0
opacity = 1.0

# Top corner rounding in scaled pixels (0.0 = sharp). Default: 6.0
border-radius = 6.0

# Border thickness in scaled pixels (0.0 = no border). Default: 1.0
border-width = 1.0

# Border color. Default: transparent (uses terminal split color).
border-color = '#44475a'

# Background color. Default: transparent (uses terminal background).
background-color = '#1e1e2e'

# Drop shadow blur radius (0.0 = no shadow). Default: 16.0
shadow-blur-radius = 16.0

# Drop shadow color. Default: '#00000066'.
shadow-color = '#00000066'

# Drop shadow offset [x, y]. Default: [0.0, -4.0]
shadow-offset = [0.0, -4.0]
```

CWD inheritance is controlled by the global `cwd` flag:

```toml
# If true, quick terminal opens in the CWD of the current pane's foreground process
cwd = true
```

All settings support hot-reload — changes to `config.toml` apply immediately
without restarting.

## Files Modified

| File | Changes |
|------|---------|
| `rio-backend/src/config/quick_terminal.rs` | New: `QuickTerminalPosition` enum (Top/Bottom/Left/Right/Center) with `border_radius()` / `is_horizontal()` / `is_vertical()` helpers; `QuickTerminalConfig` struct with `position` field and all panel appearance fields and defaults |
| `rio-backend/src/config/mod.rs` | Register `pub mod quick_terminal`, import type, add `quick_terminal` field to `Config` and `Config::default()` |
| `frontends/rioterm/src/context/grid.rs` | `QuickTerminalState<T>`, `ContextGrid::quick_terminal_config` field, `qt_panel_geometry()` helper for position-aware sizing/anchoring, `open_quick_terminal()` / `toggle_quick_terminal()` use geometry helper, `resize_quick_terminal()` supports vertical resize (top/bottom/center) and horizontal resize (left/right), position-aware `border_radius` in `extend_with_objects()`, position-aware line clipping in `qt_clip_lines_for_item()`, focus routing, dismiss on resize |
| `frontends/rioterm/src/context/mod.rs` | `quick_terminal` field on `ContextManagerConfig`, pass through all `ContextGrid::new()` call sites (×3), `toggle_quick_terminal()`, `dismiss_quick_terminal()`, PTY exit handling |
| `frontends/rioterm/src/renderer/mod.rs` | Removed main-pane clear loop (panes stay visible), `qt_bg_opacity` from config, QT content render block |
| `frontends/rioterm/src/renderer/navigation.rs` | Tab bar active-tab suppression when `qt_visible` |
| `frontends/rioterm/src/screen/mod.rs` | `ToggleQuickTerminal` dispatch (×2), `MoveDivider{Up,Down}` intercept for horizontal QT, `MoveDivider{Left,Right}` intercept for vertical QT, initial config wiring, hot-reload sync |
| `frontends/rioterm/src/bindings/mod.rs` | `Action::ToggleQuickTerminal` variant, `"togglequickterminal"` config string parser |

## References

- [iTerm2 Hotkey Window](https://iterm2.com/documentation-hotkey.html)
- [Guake terminal](https://github.com/Guake/guake)
- [Yakuake (KDE)](https://apps.kde.org/yakuake/)
