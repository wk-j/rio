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

The panel has an internal padding margin so terminal content does not overlap
the border glow accent. Main pane content is rendered normally around it.

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
    /// Raw panel origin (top-left corner of the background quad) in logical pixels.
    /// Separate from `item.position()` which is offset by the internal padding margin.
    pub panel_pos: [f32; 2],
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

### Border Glow Accent

When `[window.border-glow]` is enabled, the quick terminal renders a
top-left L-shaped corner accent in the same style as the window border glow —
a thin edge quad with shadow blur creating the glow effect, plus a chamfered
diagonal cut at the top-left corner. The accent is computed by
`compute_quick_terminal_border_glow()` in `renderer/mod.rs` and placed in
the `quick_terminal_border_glow` overlay slot in `SugarState`, rendered
between the cursor glow and window border glow overlay passes.

The glow reuses `[window.border-glow]` config with one optional override:
`quick-terminal-color` sets a separate accent color for the QT border only
(e.g. a different hue from the window glow). When not set, it falls back to
the shared `color` field.

### Internal Padding

The QT context is created with a 10 logical-pixel margin on all sides
(`x`, `top_y`, `bottom_y`). This margin offsets `item.set_position()` so
terminal content is inset from the panel edge and does not overlap the
border glow. The raw panel origin is stored separately as `panel_pos` on
`QuickTerminalState` so the background quad and glow quads still align with
the true panel boundary.

### Resize

For horizontal positions (top/bottom/center), `MoveDividerUp` / `MoveDividerDown`
are intercepted and forwarded to `resize_quick_terminal(±20.0)` which adjusts the
panel height. For vertical positions (left/right), `MoveDividerLeft` /
`MoveDividerRight` adjust the panel width instead.

The resized dimension is clamped between 10%–80% of the relevant window axis
(or 60px minimum). After resizing, the panel is reanchored to its configured
edge (or recentered for the center position).

### Rendering

Main panes render all their lines at all times — the QT is a visual overlay,
not a replacement. Sugarloaf renders ALL quads first, then ALL rich texts in
a single GPU render pass. The object list for a frame is:

```
[main pane RichText]     ← object added first
[QT background Quad]     ← rounded corners, border, drop shadow
[QT RichText]            ← terminal content

GPU draw order (within one render pass):
  1. All Quads  (main pane border quads, QT panel Quad)
  2. All RichTexts (main pane cells, then QT cells on top)
```

Because all quads render before all rich texts, the main pane's cell backgrounds
render AFTER the QT panel Quad — covering the area where the QT sits. The QT's
cell backgrounds then render on top, covering the main pane's cells in the
overlapping region. This is the same approach used by command overlays (CR-009).

Main-pane lines are **not** clipped when the QT is visible. The QT `RichText`
is always rendered with `TerminalDamage::Full` and
`bg_opacity_override = Some(config.opacity)` so default-background cells are
opaque and fully cover whatever is underneath.

The panel background quad is a plain `Quad` with no border or shadow —
visual styling is handled entirely by the border glow overlay pass.

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

Border and shadow styling is handled by `[window.border-glow]` — the quick
terminal reuses those settings with no additional appearance config.

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

# Background color. Default: transparent (uses terminal background).
background-color = '#1e1e2e'
```

Border glow appearance is configured under `[window.border-glow]`:

```toml
[window.border-glow]
enabled = true
color = "#8B5CF6"
# Optional: separate accent color for the quick terminal border only.
# Omit to inherit `color`.
quick-terminal-color = "#06B6D4"
width = 2.0
glow-radius = 8.0
glow-intensity = 0.6
animate = "none"   # "none", "pulse", "rainbow"
animate-speed = 1.0
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
| `rio-backend/src/config/quick_terminal.rs` | `QuickTerminalPosition` enum with `is_horizontal()` / `is_vertical()` helpers; `QuickTerminalConfig` with `position`, `height`, `width`, `opacity`, `background-color` only — border/shadow/radius fields removed (styling via `[window.border-glow]`) |
| `rio-backend/src/config/mod.rs` | Register `pub mod quick_terminal`, import type, add `quick_terminal` field to `Config` and `Config::default()` |
| `frontends/rioterm/src/context/grid.rs` | `QuickTerminalState<T>` with `panel_pos` field (raw panel origin separate from text-offset `item.position()`); `qt_panel_geometry()` for position-aware sizing; `open_quick_terminal()` sets text position offset by 10px internal padding; `resize_quick_terminal()` keeps `panel_pos` in sync; `extend_with_objects()` uses `panel_pos` for background quad; `quick_terminal_glow_geometry()` exposes panel bounds for glow overlay |
| `frontends/rioterm/src/context/mod.rs` | `toggle_quick_terminal()` creates QT with 10px internal margin on all sides; `current_context_with_computed_dimension()` uses stored text position directly |
| `frontends/rioterm/src/renderer/mod.rs` | `compute_quick_terminal_border_glow()` — L-shaped top+left chamfered corner accent reusing `BorderGlow` config; called each frame when QT is visible and border-glow is enabled |
| `sugarloaf/src/sugarloaf/state.rs` | `quick_terminal_border_glow: Vec<Quad>` overlay field + setter |
| `sugarloaf/src/sugarloaf.rs` | `set_quick_terminal_border_glow()` API; QT glow inserted in overlay pass between cursor glow and window border glow |
| `frontends/rioterm/src/renderer/navigation.rs` | Tab bar active-tab suppression when `qt_visible` |
| `frontends/rioterm/src/screen/mod.rs` | `ToggleQuickTerminal` dispatch, `MoveDivider` intercepts, config wiring and hot-reload sync |
| `frontends/rioterm/src/bindings/mod.rs` | `Action::ToggleQuickTerminal` variant |

## Dependencies

- CR-009 (Command Overlay Panel) — rendering approach: overlay objects (Quad +
  RichText) are added to the same object list as main pane objects. Sugarloaf
  renders all quads then all rich texts in one pass. The overlay's opaque cell
  backgrounds cover main-pane content in the overlapping region. No main-pane
  line clipping is used.
- CR-015 (Window Border Glow) — the QT border accent reuses `BorderGlow` config
  and the same `compute_border_glow` quad/shadow approach, rendered in the
  overlay pass via the `quick_terminal_border_glow` slot in `SugarState`.
- Sugarloaf `Quad` primitive

## References

- [iTerm2 Hotkey Window](https://iterm2.com/documentation-hotkey.html)
- [Guake terminal](https://github.com/Guake/guake)
- [Yakuake (KDE)](https://apps.kde.org/yakuake/)
