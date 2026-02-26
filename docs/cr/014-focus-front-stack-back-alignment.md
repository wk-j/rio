# CR-014: Focus Front / Stack Back Window Alignment

**Status:** Proposed
**Date:** 2026-02-27
**Author:** wk

## Summary

A second window alignment mode for Rio terminal: **Focus Front / Stack Back**. The focused window is brought to the **frontmost layer at full (or near-full) screen size**, while all unfocused windows are **stacked behind it in a slightly offset cascade**, remaining accessible but visually receded. Cycling focus promotes a different window to the front and pushes the previous one into the back stack. This gives a "card deck" experience — one window dominates the screen, others peek out behind it for quick switching.

## Motivation

CR-001 introduced side-by-side tiling (focused left, others stacked right). That works well for monitoring multiple terminals simultaneously but sacrifices screen real estate for the primary window. Many users prefer a **maximized primary window** workflow where the active terminal uses nearly the full screen and secondary windows stay available but out of the way — similar to macOS Stage Manager or a card stack. This mode keeps the focused window as large as possible while still providing visual cues that other windows exist behind it.

## Architecture

This feature reuses the same architecture as CR-001:

```
Application
  -> Router { routes: FxHashMap<WindowId, Route>, window_order: Vec<WindowId> }
       -> Route { RouteWindow { winit_window, screen } }
```

The key differences are:
- A new `align-mode` config option selects between `"side"` (CR-001) and `"stack"` (this CR)
- A new layout function `apply_stack_layout()` in `router/alignment.rs`
- The focused window is raised to front via `winit_window.focus_window()` + z-order manipulation
- Unfocused windows are positioned with a cascade offset behind the focused window

New `CycleStackWindowNext` / `CycleStackWindowPrev` actions and events are added for cycling focus in the stack layout, bound to `Ctrl+Shift+>` / `Ctrl+Shift+<`. The existing CR-001 side-layout bindings (`Cmd+Shift+>` / `Alt+Shift+>`) remain unchanged.

## Layout Behavior

### Focus Front + Back Stack

The focused window is centered (or positioned at a configurable origin) at a large size. Unfocused windows are stacked behind it, each offset slightly down and to the right, creating a visible cascade edge.

```
     Desktop visible area
|<----------------------------------------->|
|                                            |
|    +---------- Window C (back) ----------+ |
|    | +-------- Window B (back) --------+ | |
|    | | +------ Window A (FOCUSED) ----+| | |
|    | | |                              || | |
|    | | |    FOCUSED WINDOW            || | |
|    | | |    (nearly full screen)      || | |
|    | | |                              || | |
|    | | +------------------------------+| | |
|    | +---------------------------------+ | |
|    +-------------------------------------+ |
|                                            |
```

### Layout Rules

| Window Count | Focused Window | Unfocused Windows |
|---|---|---|
| 1 | No alignment (stays at user's position/size) | none |
| 2 | Front, centered, `align-width` ratio of screen | 1 behind, offset by `stack-offset` |
| 3 | Front, centered, `align-width` ratio of screen | 2 behind, cascaded by `stack-offset` each |
| N | Front, centered, `align-width` ratio of screen | N-1 behind, cascaded by `stack-offset` each |

Positioning details:
- **Single window:** no automatic alignment — window stays at user-defined position and size
- **Focused (2+ windows):** centered on screen at `align-width` ratio, brought to front (topmost z-order)
- **Back stack:** each unfocused window is the same size as the focused window, positioned at an increasing offset: window `i` is offset by `i * stack-offset` pixels both rightward and downward from the focused window origin
- **Z-order:** the focused window is always topmost; back-stack windows are ordered so the next-in-cycle window is directly behind the focused window, and earlier windows are further back

### Focus Cycling (Carousel)

The same ring-based cycling as CR-001. Cycling promotes the next window to the front and pushes the current one into the back stack.

Example with [A, B, C], focus A:
```
front: A    back stack: [B (offset 1), C (offset 2)]
```
Cycle next → focus B:
```
front: B    back stack: [C (offset 1), A (offset 2)]
```
Cycle next → focus C:
```
front: C    back stack: [A (offset 1), B (offset 2)]
```

### Keybindings

New dedicated keybindings for the stack layout, **additional** to the existing CR-001 side-layout bindings:

| Action | All platforms | Config string |
|---|---|---|
| Cycle stack focus to next window | `Ctrl+Shift+>` (`Ctrl+Shift+.`) | `"cyclestackwindownext"` |
| Cycle stack focus to previous window | `Ctrl+Shift+<` (`Ctrl+Shift+,`) | `"cyclestackwindowprev"` |

Existing CR-001 bindings remain unchanged:

| Action | macOS | Linux/Windows | Config string |
|---|---|---|---|
| Cycle to next window (side layout) | `Cmd+Shift+.` | `Alt+Shift+.` | `"cyclewindownext"` |
| Cycle to previous window (side layout) | `Cmd+Shift+,` | `Alt+Shift+,` | `"cyclewindowprev"` |
| Re-align all windows | `Cmd+Shift+R` | `Alt+Shift+R` | `"alignwindows"` |

**Note:** `Ctrl+Shift+>` is matched as `Ctrl+Shift+.` because `key_without_modifiers()` strips the Shift modifier. The user presses `Ctrl+Shift+>` but the key is matched as `.` with `CONTROL | SHIFT` modifiers.

### Trigger Events

Identical to CR-001 — layout recalculates on:
- Window gains focus (`WindowEvent::Focused(true)`) — unless `keyboard-only-focus` is enabled
- New window created (`RioEvent::CreateWindow`) — new window becomes focused
- Window closed — remaining windows redistribute
- Config reload — re-applies layout with updated settings
- Manual trigger (`AlignWindows` action)
- Focus cycling — side layout (`CycleWindowNext` / `CycleWindowPrev` actions)
- Focus cycling — stack layout (`CycleStackWindowNext` / `CycleStackWindowPrev` actions)

### Keyboard-Only Focus Mode

Same behavior as CR-001: when `keyboard-only-focus = true`, mouse clicks on back-stack windows give OS focus but don't trigger layout recalculation.

## Implementation

### 1. Configuration — `rio-backend/src/config/window.rs`

Add a new `align-mode` option and `stack-offset`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlignMode {
    /// CR-001: focused left, others stacked right (default)
    Side,
    /// CR-014: focused front, others stacked behind
    Stack,
}

impl Default for AlignMode {
    fn default() -> Self {
        AlignMode::Side
    }
}
```

New fields on the `Window` struct:

```rust
#[serde(default = "AlignMode::default", rename = "align-mode")]
pub align_mode: AlignMode,

#[serde(default = "default_stack_offset", rename = "stack-offset")]
pub stack_offset: u32,  // default: 30 — pixels of cascade offset per back-stack window
```

Default helper:

```rust
fn default_stack_offset() -> u32 {
    30
}
```

Full TOML example:

```toml
[window]
auto-align = true
align-mode = "stack"       # "side" (CR-001 default) or "stack" (this CR)
align-width = 0.9          # focused window width as ratio of screen
align-gap = 20             # pixels of margin around the focused window
stack-offset = 30          # pixels of cascade offset per back-stack window
keyboard-only-focus = true
```

### 2. Layout Engine — `router/alignment.rs`

Add a new `apply_stack_layout()` function alongside the existing `apply_layout()`:

```rust
/// Apply focus-front / stack-back layout.
///
/// The focused window is centered on screen at `align_width` ratio
/// and brought to the front. Unfocused windows are the same size,
/// cascaded behind with increasing offset.
pub fn apply_stack_layout(
    routes: &mut FxHashMap<WindowId, Route>,
    focused_id: WindowId,
    window_order: &[WindowId],
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    stack_offset: u32,
) {
    let len = window_order.len();
    if len < 2 {
        return;
    }

    let decoration_height = routes
        .values()
        .next()
        .map(|route| {
            let outer = route.window.winit_window.outer_size();
            let inner = route.window.winit_window.inner_size();
            let scale = route.window.winit_window.scale_factor();
            ((outer.height.saturating_sub(inner.height)) as f64
                / scale) as u32
        })
        .unwrap_or(0);

    // Compute focused window slot (centered)
    let ratio = align_width.clamp(0.1, 1.0);
    let w = (screen.width.saturating_sub(gap * 2) as f32 * ratio)
        as u32;
    let h = screen
        .height
        .saturating_sub(gap * 2 + decoration_height);
    let base_x = screen.x
        + ((screen.width.saturating_sub(w)) / 2) as i32;
    let base_y = screen.y + gap as i32;

    // Collect unfocused windows in ring order
    let focused_idx = window_order
        .iter()
        .position(|id| *id == focused_id)
        .unwrap_or(0);

    let mut stack_windows: Vec<WindowId> =
        Vec::with_capacity(len - 1);
    for step in 1..len {
        let idx = (focused_idx + step) % len;
        stack_windows.push(window_order[idx]);
    }

    // Position back-stack windows FIRST (furthest back first)
    // so that when we position the focused window last,
    // it ends up on top in z-order.
    for (i, id) in stack_windows.iter().enumerate().rev() {
        let depth = (i + 1) as i32;
        let offset = depth * stack_offset as i32;
        let slot = WindowSlot {
            x: base_x + offset,
            y: base_y + offset,
            width: w,
            height: h,
        };
        if let Some(route) = routes.get_mut(id) {
            apply_slot(route, &slot);
        }
    }

    // Position and raise focused window (last = topmost)
    let focused_slot = WindowSlot {
        x: base_x,
        y: base_y,
        width: w,
        height: h,
    };
    if let Some(route) = routes.get_mut(&focused_id) {
        apply_slot(route, &focused_slot);
        route.window.winit_window.focus_window();
    }
}

/// Cycle focus in stack mode: promote next/prev window to front.
pub fn cycle_focus_stack(
    routes: &mut FxHashMap<WindowId, Route>,
    window_order: &[WindowId],
    current_focused: WindowId,
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    stack_offset: u32,
    reverse: bool,
) -> Option<WindowId> {
    if window_order.len() < 2 {
        return None;
    }

    let current_idx = window_order
        .iter()
        .position(|id| *id == current_focused)
        .unwrap_or(0);

    let next_idx = if reverse {
        if current_idx == 0 {
            window_order.len() - 1
        } else {
            current_idx - 1
        }
    } else {
        (current_idx + 1) % window_order.len()
    };

    let new_focused = window_order[next_idx];

    apply_stack_layout(
        routes,
        new_focused,
        window_order,
        screen,
        gap,
        align_width,
        stack_offset,
    );

    Some(new_focused)
}
```

### 3. Events — `rio-backend/src/event/mod.rs`

```rust
pub enum RioEvent {
    // ...existing variants...

    /// Cycle focus to the next window using stack layout.
    CycleStackWindowNext,

    /// Cycle focus to the previous window using stack layout.
    CycleStackWindowPrev,
}
```

Display impl additions:

```rust
RioEvent::CycleStackWindowNext => write!(f, "CycleStackWindowNext"),
RioEvent::CycleStackWindowPrev => write!(f, "CycleStackWindowPrev"),
```

### 4. Actions — `frontends/rioterm/src/bindings/mod.rs`

```rust
pub enum Action {
    // ...existing variants...

    /// Cycle focus to next window using stack (front/back) layout
    CycleStackWindowNext,

    /// Cycle focus to previous window using stack (front/back) layout
    CycleStackWindowPrev,
}
```

String mappings:

```rust
"cyclestackwindownext" => Some(Action::CycleStackWindowNext),
"cyclestackwindowprev" => Some(Action::CycleStackWindowPrev),
```

### 5. Default Keybindings — `frontends/rioterm/src/bindings/mod.rs`

Added to **all platform** binding blocks (macOS, Linux x11, Linux Wayland):

```rust
// Stack layout focus cycling: Ctrl+Shift+> and Ctrl+Shift+<
// key_without_modifiers() strips shift, so we match on "." and ","
".", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::VI, ~BindingMode::SEARCH; Action::CycleStackWindowPrev;
",", ModifiersState::CONTROL | ModifiersState::SHIFT, ~BindingMode::VI, ~BindingMode::SEARCH; Action::CycleStackWindowNext;
```

### 6. Screen Dispatch — `frontends/rioterm/src/screen/mod.rs`

Action handling (general + leader-menu):

```rust
Act::CycleStackWindowNext => {
    self.context_manager.cycle_stack_window_next();
}
Act::CycleStackWindowPrev => {
    self.context_manager.cycle_stack_window_prev();
}
```

### 7. ContextManager — `frontends/rioterm/src/context/mod.rs`

```rust
pub fn cycle_stack_window_next(&self) {
    self.event_proxy
        .send_event(RioEvent::CycleStackWindowNext, self.window_id);
}

pub fn cycle_stack_window_prev(&self) {
    self.event_proxy
        .send_event(RioEvent::CycleStackWindowPrev, self.window_id);
}
```

### 8. Application Integration — `application.rs`

The existing `align_windows_with()` uses the `align-mode` config to choose which layout to apply on automatic triggers (window create/close, config reload, focus change). The new `cycle_stack_window_focus()` method always uses the stack layout regardless of config, since the keybinding explicitly requests it.

```rust
fn align_windows_with(
    &mut self,
    override_focused: Option<WindowId>,
) {
    if self.router.window_order.len() < 2 {
        return;
    }

    let focused_id = match override_focused
        .or_else(|| self.router.get_focused_route())
        .or_else(|| self.router.window_order.last().copied())
    {
        Some(id) => id,
        None => return,
    };

    let screen = match self.router.routes.get(&focused_id) {
        Some(route) => {
            crate::router::alignment::get_available_screen_area(
                &route.window.winit_window,
            )
        }
        None => return,
    };

    let screen = match screen {
        Some(s) => s,
        None => return,
    };

    match self.config.window.align_mode {
        rio_backend::config::window::AlignMode::Side => {
            crate::router::alignment::apply_layout(
                &mut self.router.routes,
                focused_id,
                &self.router.window_order,
                &screen,
                self.config.window.peek_width,
                self.config.window.align_gap,
                self.config.window.align_width,
            );
        }
        rio_backend::config::window::AlignMode::Stack => {
            crate::router::alignment::apply_stack_layout(
                &mut self.router.routes,
                focused_id,
                &self.router.window_order,
                &screen,
                self.config.window.align_gap,
                self.config.window.align_width,
                self.config.window.stack_offset,
            );
        }
    }
}

/// Cycle focus using the stack (front/back) layout.
/// Called by CycleStackWindowNext / CycleStackWindowPrev events.
/// Always uses stack layout regardless of align-mode config.
fn cycle_stack_window_focus(&mut self, reverse: bool) {
    let focused_id = match self
        .router
        .get_focused_route()
        .or_else(|| self.router.window_order.last().copied())
    {
        Some(id) => id,
        None => return,
    };

    let screen = match self.router.routes.get(&focused_id) {
        Some(route) => {
            crate::router::alignment::get_available_screen_area(
                &route.window.winit_window,
            )
        }
        None => return,
    };

    let screen = match screen {
        Some(s) => s,
        None => return,
    };

    self.keyboard_triggered_focus = true;

    let window_order = self.router.window_order.clone();
    crate::router::alignment::cycle_focus_stack(
        &mut self.router.routes,
        &window_order,
        focused_id,
        &screen,
        self.config.window.align_gap,
        self.config.window.align_width,
        self.config.window.stack_offset,
        reverse,
    );
}
```

Event dispatch in `user_event()`:

```rust
RioEventType::Rio(RioEvent::CycleStackWindowNext) => {
    if self.config.window.auto_align {
        self.cycle_stack_window_focus(false);
    }
}
RioEventType::Rio(RioEvent::CycleStackWindowPrev) => {
    if self.config.window.auto_align {
        self.cycle_stack_window_focus(true);
    }
}
```

The existing `cycle_window_focus()` for CR-001 side-layout events remains unchanged.

### 9. Platform Notes

**macOS z-order:** `focus_window()` via winit brings a window to the front on macOS. The back-stack windows are positioned first (furthest back to nearest back) so that the OS window stacking order matches the visual cascade. The focused window is positioned last and `focus_window()` is called on it, ensuring it is the topmost window.

**Window ordering guarantee:** On macOS, calling `set_outer_position()` on a window may bring it forward. To ensure correct z-order, back-stack windows are positioned in reverse order (furthest back first), and the focused window is positioned and focused last. If this proves insufficient, we may need to use `NSWindow::orderWindow:relativeTo:` via raw platform access.

**Same `core-graphics` dependency** as CR-001 for macOS screen detection.

## Visual Examples

### 3 windows, Window A focused (align-width: 0.9, gap: 20, stack-offset: 30):

```
  +--------------------------------------------------+
  |gap                                            gap|
  |   +------- Window C (offset 60,60) --------+     |
  |   | +------ Window B (offset 30,30) -----+ |     |
  |   | | +---- Window A (FOCUSED) ---------+ | |     |
  |   | | |                                 | | |     |
  |   | | |      FOCUSED WINDOW             | | |     |
  |   | | |      90% screen width           | | |     |
  |   | | |      centered                   | | |     |
  |   | | |                                 | | |     |
  |   | | +---------------------------------+ | |     |
  |   | +-----------------------------------+ |       |
  |   +---------------------------------------+       |
  |                                                   |
```

### Cycle next → focus B:

```
  +--------------------------------------------------+
  |   +------- Window A (offset 60,60) --------+     |
  |   | +------ Window C (offset 30,30) -----+ |     |
  |   | | +---- Window B (FOCUSED) ---------+ | |     |
  |   | | |                                 | | |     |
  |   | | |      FOCUSED WINDOW             | | |     |
  |   | | |      90% screen width           | | |     |
  |   | | |                                 | | |     |
  |   | | +---------------------------------+ | |     |
  |   | +-----------------------------------+ |       |
  |   +---------------------------------------+       |
```

### Cycle next → focus C:

```
  +--------------------------------------------------+
  |   +------- Window B (offset 60,60) --------+     |
  |   | +------ Window A (offset 30,30) -----+ |     |
  |   | | +---- Window C (FOCUSED) ---------+ | |     |
  |   | | |                                 | | |     |
  |   | | |      FOCUSED WINDOW             | | |     |
  |   | | |      90% screen width           | | |     |
  |   | | |                                 | | |     |
  |   | | +---------------------------------+ | |     |
  |   | +-----------------------------------+ |       |
  |   +---------------------------------------+       |
```

### Single window:

```
  Window stays at user's position/size — no automatic alignment.
```

## Implementation Phases

1. Add `AlignMode` enum, `align_mode` field, and `stack_offset` to `rio-backend/src/config/window.rs`
2. Add `CycleStackWindowNext` / `CycleStackWindowPrev` to `RioEvent` enum in `rio-backend/src/event/mod.rs`
3. Add `CycleStackWindowNext` / `CycleStackWindowPrev` to `Action` enum + string parsing in `frontends/rioterm/src/bindings/mod.rs`
4. Add default keybindings `Ctrl+Shift+.` / `Ctrl+Shift+,` to all platform binding blocks
5. Add `apply_stack_layout()` and `cycle_focus_stack()` to `router/alignment.rs`
6. Add `cycle_stack_window_next()` / `cycle_stack_window_prev()` to `ContextManager`
7. Add `Act::CycleStackWindowNext` / `Act::CycleStackWindowPrev` dispatch in `Screen`
8. Add `cycle_stack_window_focus()` to `Application` and handle the new events in `user_event()`
9. Modify `align_windows_with()` to dispatch based on `align_mode` config
10. Test z-order behavior on macOS — ensure focused window is always topmost
11. Test with 2, 3, 4+ windows to verify cascade offset calculation
12. Test that `Ctrl+Shift+>` / `<` always uses stack layout, `Cmd+Shift+>` / `<` always uses side layout
13. If z-order is unreliable via winit, add platform-specific `NSWindow::orderWindow:relativeTo:` fallback

## File Changes

| File | Change |
|---|---|
| `rio-backend/src/config/window.rs` | Add `AlignMode` enum, `align_mode` field, `stack_offset` field, defaults |
| `rio-backend/src/event/mod.rs` | Add `CycleStackWindowNext`, `CycleStackWindowPrev` variants to `RioEvent` |
| `frontends/rioterm/src/bindings/mod.rs` | Add `CycleStackWindowNext`, `CycleStackWindowPrev` to `Action` enum, string parsing, and default keybindings (`Ctrl+Shift+.` / `Ctrl+Shift+,`) |
| `frontends/rioterm/src/router/alignment.rs` | Add `apply_stack_layout()`, `cycle_focus_stack()` |
| `frontends/rioterm/src/context/mod.rs` | Add `cycle_stack_window_next()`, `cycle_stack_window_prev()` methods |
| `frontends/rioterm/src/screen/mod.rs` | Add dispatch for `Act::CycleStackWindowNext`, `Act::CycleStackWindowPrev` |
| `frontends/rioterm/src/application.rs` | Add `cycle_stack_window_focus()`, handle new events, modify `align_windows_with()` to match on `align_mode` |

## Dependencies

- Same as CR-001: `winit`/`rio-window`, `core-graphics` (macOS)
- No new external dependencies
