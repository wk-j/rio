# CR-016: Quick Terminal

**Status:** Implemented
**Date:** 2026-03-01
**Author:** wk

## Summary

Implement a quick terminal overlay — a persistent PTY that slides over all existing
panes with a single keybinding. Inspired by iTerm2's Hotkey Window and Guake.
Toggling the same key shows/hides the overlay without destroying the shell session.
The overlay inherits the CWD of the previously focused pane.

## Motivation

1. **Fast context switching** — reach a scratch shell instantly without leaving the
   current workflow or creating a new tab.
2. **Session persistence** — the PTY stays alive when hidden; processes keep running.
3. **CWD inheritance** — opens in the working directory of the active pane so
   clipboard-less file path reuse is immediate.
4. **Resizable** — height is adjustable with the existing divider resize keys.

## User Flow

```
1. User presses ToggleQuickTerminal binding (e.g. Ctrl+`)
2. Quick terminal overlay appears covering the full pane area
3. User runs commands; pane content beneath is preserved unchanged
4. Press binding again → overlay hides, focus returns to previous pane
5. Press binding again → overlay re-appears with the same shell, same history
6. User types `exit` → shell exits, overlay is destroyed; pane regains focus
7. Window resize → overlay auto-dismisses (hides, not destroyed)
8. Switching tab / creating split → overlay auto-dismisses
```

### Visual Example

```
+--------------------------------------------------+
| $ cargo build                                     |   ← main pane (hidden under QT)
|    Compiling rio v0.2.0                          |
|    ...                                           |
+==================================================+   ← separator line (border_color)
| $ █                                              |   ← quick terminal overlay
|                                                  |
|                                                  |
+--------------------------------------------------+
```

The separator is a thin horizontal quad rendered in `border_color`. The overlay
background is opaque so main pane content does not bleed through.

## Architecture

### State

```rust
/// State for the quick terminal overlay pane.
/// The quick terminal is rendered as an overlay on top of main panes —
/// main pane dimensions are never modified.
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
                └─► visible = false ──► show (visible=true, save current focus, update dims)
```

### Resize

The QT is sized to the full window minus margins when shown. The divider resize
actions (`MoveDividerUp` / `MoveDividerDown`) are intercepted in `screen/mod.rs`
when the QT is visible and forwarded to `resize_quick_terminal(±20.0)` instead
of the normal split resize path. Height is clamped between 10%–80% of window height.

### Rendering — Two-Pass

1. **Main pane pass** — when `is_quick_terminal_visible()` is true, every main pane
   rich text is cleared (`content.clear()`) and skipped. Main pane PTYs are not
   re-rendered; their dimension data is untouched.
2. **Overlay pass** — QT content is always rendered with `TerminalDamage::Full` and
   `bg_opacity_override = Some(1.0)` (opaque backgrounds) so default-background
   cells fully occlude anything beneath them.
3. **Object list** — three GPU objects are appended after all main-pane objects:
   - Opaque background `Quad` (covers the separator + content area)
   - Separator `Quad` (thin, `border_color`)
   - QT `RichText` object

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
i == current`), reflecting that the overlay is transient and does not belong to any
specific split layout.

## Configuration

No dedicated configuration section. The feature integrates with the existing
keybinding system:

```toml
[bindings]
keys = [
    { key = "`", mods = "Control", action = "ToggleQuickTerminal" },
]
```

CWD inheritance is controlled by the global `cwd` flag:

```toml
# If true, quick terminal opens in the CWD of the current pane's foreground process
cwd = true
```

## Files Modified

| File | Changes |
|------|---------|
| `frontends/rioterm/src/context/grid.rs` | `QuickTerminalState<T>`, `ContextGrid::quick_terminal` field, `open_quick_terminal()`, `toggle_quick_terminal()`, `resize_quick_terminal()`, focus routing in `current()` / `current_mut()` / `current_position()` / `current_context_with_computed_dimension()`, object list building in `extend_with_objects()`, dismiss on `resize()` |
| `frontends/rioterm/src/context/mod.rs` | `ContextManager::toggle_quick_terminal()`, `dismiss_quick_terminal()` (8 call sites), PTY exit handling in `should_close_context_manager()` |
| `frontends/rioterm/src/renderer/mod.rs` | Two-pass rendering: clear-main-panes loop, QT content render block, `bg_opacity_override = Some(1.0)` |
| `frontends/rioterm/src/renderer/navigation.rs` | Tab bar active-tab suppression when `qt_visible` |
| `frontends/rioterm/src/screen/mod.rs` | `Act::ToggleQuickTerminal` dispatch (×2), `Act::MoveDivider{Up,Down}` intercept for QT resize |
| `frontends/rioterm/src/bindings/mod.rs` | `Action::ToggleQuickTerminal` variant, `"togglequickterminal"` config string parser |

## References

- [iTerm2 Hotkey Window](https://iterm2.com/documentation-hotkey.html)
- [Guake terminal](https://github.com/Guake/guake)
- [Yakuake (KDE)](https://apps.kde.org/yakuake/)
