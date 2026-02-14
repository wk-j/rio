# CR-001: Auto Window Alignment (Focus-Centered Tiling)

**Status:** Proposed
**Date:** 2026-02-14
**Author:** wk

## Summary

Automatically arrange Rio terminal windows in a focus-centered layout. The **active (focused) window always occupies the center** of the screen at full size. **Unfocused windows are positioned beyond the desktop edges** (off-screen) so they are hidden but still alive. When focus cycles to another window, it slides to center and the previously focused window moves off-screen. This gives a clean single-window experience while maintaining instant access to all windows via focus cycling.

## Motivation

Currently, new Rio windows open at default OS positions (often overlapping). Users who work with multiple terminal windows must manually resize and position them every time. A focus-centered tiling layout keeps the most important window (the one you're working in) front and center, with secondary windows visible but out of the way — no external window manager needed.

## Current Architecture

All Rio windows run in a single process sharing one event loop:

```
Application
  -> Router { routes: FxHashMap<WindowId, Route> }
       -> Route { RouteWindow { winit_window, screen } }
```

- `Router.routes` holds every open window keyed by `WindowId`
- `route.window.is_focused` tracks which window is active (`application.rs:1292`)
- `EventProxy` sends events between windows via the shared event loop
- `Application::user_event()` dispatches events and can iterate all routes
- Window creation happens in `Router::create_window()` (`router/mod.rs:417`)

Since all windows are in-process, no IPC is needed.

## Proposed Behavior

### Layout: Focus-Centered with Edge Peek

The focused window is centered on screen at full size. Unfocused windows are positioned mostly beyond the desktop edges, with a small **peek strip** visible — just enough to indicate their presence and allow click-to-focus.

```
         Desktop visible area
|<----------------------------------------->|
|                                           |
|]  |        FOCUSED WINDOW          |  [   |
|]  |         (full center)          |  [   |
|]  |                                |  [   |
|]  |                                |  [   |
peek                                   peek
Window A                               Window C
(~50px visible)                        (~50px visible)
```

The peek strip acts as a visual hint: "there are more windows to the left/right." Clicking the peek area or using a keybinding brings that window to center.

### Detailed Layout Rules

| Window Count | Focused Window | Unfocused Windows |
|---|---|---|
| 1 | Full screen | none |
| 2 | Full screen | 1 peeking from left edge |
| 3 | Full screen | 1 peeking left, 1 peeking right |
| 4 | Full screen | 2 stacked peeking left, 1 peeking right |
| N | Full screen | alternating left/right, stacked if multiple per side |

All windows are the same size as the focused window. Positioning:
- **Focused:** `x = screen.x` (fully visible)
- **Peek left:** `x = screen.x - window.width + peek_width` (only rightmost `peek_width` pixels visible)
- **Peek right:** `x = screen.x + screen.width - peek_width` (only leftmost `peek_width` pixels visible)

When multiple windows peek from the same side, they stack vertically (each gets a fraction of the screen height) so all peek strips are visible.

Unfocused windows keep their PTY sessions running, so content is always up to date when cycled into view.

### Focus Cycling (Keyboard-Centric)

This feature is **keyboard-centric**. Since unfocused windows are mostly off-screen, clicking them is unreliable across platforms. All window switching is done via keybindings:

| Action | Default Keybinding (macOS) | Default (Linux/Windows) |
|---|---|---|
| Cycle to next window | `Cmd+`` ` | `Alt+`` ` |
| Cycle to previous window | `Cmd+Shift+`` ` | `Alt+Shift+`` ` |
| Focus window by number | `Cmd+Ctrl+1/2/3/...` | `Alt+Ctrl+1/2/3/...` |
| Re-align all windows | `Cmd+Shift+R` | `Ctrl+Shift+R` |

On focus change:
1. Previously focused window slides to a peek position (left or right edge)
2. Newly focused window slides to center
3. Other unfocused windows rebalance their peek positions

The cycle order is based on window creation order (stable, predictable). Direct selection by number (`Cmd+Ctrl+1`) uses the same ordering.

If the user happens to click a peek strip and the OS delivers the focus event, it is handled the same way — the clicked window moves to center. But this is not the expected primary workflow.

### Trigger Events

Layout recalculates when:
- A window gains focus (`WindowEvent::Focused(true)`)
- A new window is created (new window gets focus and becomes center)
- A window is closed (remaining windows redistribute)

## Design

### 1. Screen Geometry

```rust
// router/alignment.rs (new file)
pub struct ScreenArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub fn get_available_screen_area(window: &Window) -> ScreenArea {
    // Use winit's MonitorHandle to get work area
    // Accounts for menu bar, dock, taskbar
}
```

### 2. Layout Calculator (Edge Peek)

```rust
pub struct WindowSlot {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub enum PeekSide {
    Left,
    Right,
}

/// Position for the focused window: full screen.
pub fn focused_slot(screen: &ScreenArea) -> WindowSlot {
    WindowSlot {
        x: screen.x,
        y: screen.y,
        width: screen.width,
        height: screen.height,
    }
}

/// Position for an unfocused window peeking from an edge.
/// `stack_index` and `stack_count` control vertical stacking when
/// multiple windows peek from the same side.
pub fn peek_slot(
    screen: &ScreenArea,
    side: PeekSide,
    peek_width: u32,
    stack_index: usize,
    stack_count: usize,
) -> WindowSlot {
    let x = match side {
        // Window extends to the left; only the rightmost `peek_width` is on screen
        PeekSide::Left => screen.x - screen.width as i32 + peek_width as i32,
        // Window extends to the right; only the leftmost `peek_width` is on screen
        PeekSide::Right => screen.x + screen.width as i32 - peek_width as i32,
    };

    // Vertical stacking: divide screen height among windows on the same side
    let slot_height = screen.height / stack_count.max(1) as u32;
    let y = screen.y + (stack_index as u32 * slot_height) as i32;

    WindowSlot {
        x,
        y,
        width: screen.width,
        height: slot_height,
    }
}
```

### 3. Layout Application

```rust
/// Maintain a stable ordering of window IDs for consistent cycling.
/// Stored in Router as: pub window_order: Vec<WindowId>

pub fn apply_layout(
    routes: &mut FxHashMap<WindowId, Route>,
    focused_id: WindowId,
    window_order: &[WindowId],
    screen: &ScreenArea,
    peek_width: u32,
) {
    // Move focused window to center (full screen)
    if let Some(route) = routes.get_mut(&focused_id) {
        let slot = focused_slot(screen);
        route.window.winit_window.set_outer_position(
            PhysicalPosition::new(slot.x, slot.y)
        );
        let _ = route.window.winit_window.request_inner_size(
            PhysicalSize::new(slot.width, slot.height)
        );
    }

    // Distribute unfocused windows to left/right peek positions
    let mut left_windows: Vec<WindowId> = Vec::new();
    let mut right_windows: Vec<WindowId> = Vec::new();

    for id in window_order {
        if *id == focused_id {
            continue;
        }
        if left_windows.len() <= right_windows.len() {
            left_windows.push(*id);
        } else {
            right_windows.push(*id);
        }
    }

    // Position left-side peek windows (stacked vertically)
    for (i, id) in left_windows.iter().enumerate() {
        let slot = peek_slot(screen, PeekSide::Left, peek_width,
                             i, left_windows.len());
        if let Some(route) = routes.get_mut(id) {
            route.window.winit_window.set_outer_position(
                PhysicalPosition::new(slot.x, slot.y)
            );
            let _ = route.window.winit_window.request_inner_size(
                PhysicalSize::new(slot.width, slot.height)
            );
        }
    }

    // Position right-side peek windows (stacked vertically)
    for (i, id) in right_windows.iter().enumerate() {
        let slot = peek_slot(screen, PeekSide::Right, peek_width,
                             i, right_windows.len());
        if let Some(route) = routes.get_mut(id) {
            route.window.winit_window.set_outer_position(
                PhysicalPosition::new(slot.x, slot.y)
            );
            let _ = route.window.winit_window.request_inner_size(
                PhysicalSize::new(slot.width, slot.height)
            );
        }
    }
}

/// Cycle focus to the next/previous window in order.
pub fn cycle_focus(
    routes: &mut FxHashMap<WindowId, Route>,
    window_order: &[WindowId],
    current_focused: WindowId,
    screen: &ScreenArea,
    peek_width: u32,
    reverse: bool,
) -> WindowId {
    let current_idx = window_order.iter()
        .position(|id| *id == current_focused)
        .unwrap_or(0);
    let next_idx = if reverse {
        if current_idx == 0 { window_order.len() - 1 }
        else { current_idx - 1 }
    } else {
        (current_idx + 1) % window_order.len()
    };
    let new_focused = window_order[next_idx];

    // Focus the new window
    if let Some(route) = routes.get(&new_focused) {
        route.window.winit_window.focus_window();
    }

    apply_layout(routes, new_focused, window_order, screen, peek_width);
    new_focused
}
```

### 4. Integration Points

**Focus change** (`application.rs:1286`):
When `WindowEvent::Focused(true)` fires, call `apply_focus_centered_layout()` with the newly focused `WindowId`.

**Window creation** (`router/mod.rs:417`):
After inserting the new route, the new window gains focus, triggering the layout.

**Window close** (`application.rs`, route removal):
After removing a route, recalculate layout for the remaining windows with the currently focused one as center.

**New `RioEvent` variant:**
```rust
RioEvent::RealignWindows
```

**New actions:**
```rust
Act::RealignWindows       // Force re-alignment (clears pinned state)
Act::CycleWindowNext      // Cycle focus to next window
Act::CycleWindowPrev      // Cycle focus to previous window
```

**Default keybindings:**
- `Cmd+`` ` → `CycleWindowNext`
- `Cmd+Shift+`` ` → `CycleWindowPrev`
- `Cmd+Shift+R` → `RealignWindows`

### 5. Configuration

```toml
[window]
auto-align = true
peek-width = 50             # pixels of unfocused window visible at screen edge
```

### 6. Pinned Window Tracking

```rust
pub struct RouteWindow<'a> {
    // ... existing fields ...
    pub is_pinned: bool,
}
```

- Manual move/resize sets `is_pinned = true` — window is excluded from auto-layout
- `RealignWindows` action clears all pinned states and re-tiles

## Visual Example

```
Initial state (3 windows, Window B focused):

  Desktop edge                          Desktop edge
  |                                              |
  |]|           Window B (FOCUSED)            |[|
  |]|                                         |[|
  |]|           full screen, centered         |[|
  |]|                                         |[|
  |]|                                         |[|
  | |                                         | |
  A                                             C
  peek                                        peek
  50px                                        50px

User presses Cmd+` (cycle next → focus Window C):

  |                                              |
  |]|           Window C (FOCUSED)            |[|
  |]|                                         |[|
  |]|           full screen, centered         |[|
  |]|                                         |[|
  |]|                                         |[|
  | |                                         | |
  B                                             A
  peek                                        peek

User presses Cmd+` again (cycle next → focus Window A):

  |                                              |
  |]|           Window A (FOCUSED)            |[|
  |]|                                         |[|
  |]|           full screen, centered         |[|
  |]|                                         |[|
  |]|                                         |[|
  | |                                         | |
  C                                             B
  peek                                        peek

With 4 windows (D focused), left side stacks vertically:

  |                                              |
  |]|                                         |[|
  A |           Window D (FOCUSED)            |[|
  |]|                                         | |
  ---           full screen, centered         |[|
  |]|                                         C |
  B |                                         |[|
  |]|                                         | |
  peek                                        peek
  (A and B                                    (C alone,
   stacked)                                    full height)
```

## Implementation Plan

1. **Phase 1:** Add `router/alignment.rs` with screen geometry and focus-centered layout calculator
2. **Phase 2:** Hook into `WindowEvent::Focused(true)` to trigger layout on focus change
3. **Phase 3:** Hook into `create_window()` and window close for layout recalculation
4. **Phase 4:** Add `RealignWindows` and `CycleWindowFocus` keybindings
5. **Phase 5:** Add config options (`auto-align`, `center-ratio`, `align-gap`)
6. **Phase 6:** Pinned window support and multi-monitor awareness

## Open Questions

- Should layout transitions be animated (smooth slide) or instant snap?
- Debounce focus events? (Rapid cycling could cause layout thrashing)
- Should peek strips show a visual indicator (e.g., tab title, window number)?
- How to handle macOS native tabs (multiple tabs = one window)?
- Should peek windows be dimmed/blurred to emphasize the focused center?
- Should the peek width scale with display DPI?
- Can the user click the peek strip to focus that window, or only use keybindings?

## Dependencies

- `winit` / `rio-window`: `Window::set_outer_position()`, `Window::request_inner_size()`, `MonitorHandle`
- `route.window.is_focused` (already exists at `router/mod.rs:493`)
- No external dependencies required
- No IPC needed (all in-process)
