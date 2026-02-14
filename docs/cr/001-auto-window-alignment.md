# CR-001: Auto Window Alignment (Focus-Centered Tiling)

**Status:** Implemented
**Date:** 2026-02-14
**Author:** wk

## Summary

Automatically arrange Rio terminal windows in a focus-centered layout. The **focused window occupies the left portion** of the screen at a configurable width ratio (`align-width`). **Unfocused windows are stacked vertically on the right side**, sharing the remaining screen space. Cycling focus rotates which window sits on the left — others restack on the right. This gives a clean primary-window experience with secondary windows always visible, no external window manager needed.

## Motivation

Currently, new Rio windows open at default OS positions (often overlapping). Users who work with multiple terminal windows must manually resize and position them every time. A focus-centered tiling layout keeps the most important window (the one you're working in) large and prominent, with secondary windows visible and stacked on the side for quick reference.

## Architecture

All Rio windows run in a single process sharing one event loop:

```
Application
  -> Router { routes: FxHashMap<WindowId, Route>, window_order: Vec<WindowId> }
       -> Route { RouteWindow { winit_window, screen } }
```

- `Router.routes` holds every open window keyed by `WindowId`
- `Router.window_order` maintains stable creation-order for cycling
- `route.window.is_focused` tracks which window is active
- `EventProxy` sends events between windows via the shared event loop
- `Application::user_event()` dispatches events and can iterate all routes
- Window creation happens in `Router::create_window()` which returns the new `WindowId`

Since all windows are in-process, no IPC is needed.

## Layout Behavior

### Focus + Right-Side Stack

The focused window sits on the left at `align-width` ratio. All unfocused windows stack vertically on the right, dividing the remaining space equally in height with gaps between them.

```
     Desktop visible area (2048px)
|<----------------------------------------->|
|gap|                        |gap|          |gap|
|   |    FOCUSED WINDOW      |   | Window A |   |
|   |    (align-width: 80%)  |   |----------|   |
|   |    1638px wide         |   | Window C |   |
|   |                        |   |          |   |
|   |                        |   | ~370px   |   |
```

### Layout Rules

| Window Count | Focused Window | Unfocused Windows |
|---|---|---|
| 1 | Centered at `align-width` ratio | none |
| 2 | Left-aligned at `align-width` ratio | 1 stacked right, full height |
| 3 | Left-aligned at `align-width` ratio | 2 stacked right, half height each |
| N | Left-aligned at `align-width` ratio | N-1 stacked right, height = available / (N-1) |

Positioning details:
- **Focused (1 window):** centered horizontally, `x = screen.x + (screen.width - w) / 2`
- **Focused (2+ windows):** left-aligned, `x = screen.x + gap`
- **Stacked:** `x = focused.x + focused.width + gap`, sharing remaining width to screen edge minus gap
- **Stacked height:** `(screen.height - 2*gap - (N-2)*gap) / (N-1)` per window, with gap between each

Unfocused windows maintain their PTY sessions, so content is always current when cycled into focus.

### Focus Cycling (Carousel)

Windows form a ring in creation order. Cycling advances the focus forward or backward through the ring. The focused window always moves to the left position; all others restack on the right.

Example with [A, B, C], focus B:
```
left: B (80%)  right stack: [C, A]
```
Cycle next → focus C:
```
left: C (80%)  right stack: [A, B]
```
Cycle next → focus A:
```
left: A (80%)  right stack: [B, C]
```

### Keybindings

| Action | macOS | Linux/Windows | Config string |
|---|---|---|---|
| Cycle to next window | `Cmd+Shift+.` | `Alt+Shift+.` | `"cyclewindownext"` |
| Cycle to previous window | `Cmd+Shift+,` | `Alt+Shift+,` | `"cyclewindowprev"` |
| Re-align all windows | `Cmd+Shift+R` | `Alt+Shift+R` | `"alignwindows"` |

**Note:** Keybindings use base characters `.` and `,` (not `>` and `<`) because `key_without_modifiers()` strips the Shift modifier. The user presses `Cmd+Shift+>` but the key is matched as `Cmd+Shift+.`.

### Trigger Events

Layout recalculates on:
- Window gains focus (`WindowEvent::Focused(true)`)
- New window created (`RioEvent::CreateWindow`) — new window becomes focused
- Window closed — remaining windows redistribute
- Config reload — re-applies layout with updated settings
- Manual trigger (`AlignWindows` action)
- Focus cycling (`CycleWindowNext` / `CycleWindowPrev` actions)

## Implementation

### 1. Layout Engine — `router/alignment.rs`

```rust
pub struct ScreenArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct WindowSlot {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Get screen area using CGDisplay::main() on macOS (avoids NSScreen crash)
/// or current_monitor() on other platforms.
pub fn get_available_screen_area(window: &Window) -> Option<ScreenArea>;

/// Focused window position. Centered if alone, left-aligned if has peers.
pub fn focused_slot(
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    has_peers: bool,
) -> WindowSlot;

/// Apply layout: focused left, unfocused stacked right.
pub fn apply_layout(
    routes: &mut FxHashMap<WindowId, Route>,
    focused_id: WindowId,
    window_order: &[WindowId],
    screen: &ScreenArea,
    _peek_width: u32,
    gap: u32,
    align_width: f32,
);

/// Cycle focus to next/previous window and re-layout.
pub fn cycle_focus(
    routes: &mut FxHashMap<WindowId, Route>,
    window_order: &[WindowId],
    current_focused: WindowId,
    screen: &ScreenArea,
    peek_width: u32,
    gap: u32,
    align_width: f32,
    reverse: bool,
) -> Option<WindowId>;
```

All positions use `LogicalPosition` / `LogicalSize` (not Physical) because macOS CGDisplay returns logical points — using Physical on Retina would double-divide by scale factor.

### 2. Events — `rio-backend/src/event/mod.rs`

```rust
pub enum RioEvent {
    // ...existing variants...
    AlignWindows,
    CycleWindowNext,
    CycleWindowPrev,
}
```

### 3. Actions — `frontends/rioterm/src/bindings/mod.rs`

```rust
pub enum Action {
    // ...existing variants...
    CycleWindowNext,
    CycleWindowPrev,
    AlignWindows,
}
```

String mappings: `"cyclewindownext"`, `"cyclewindowprev"`, `"alignwindows"`.

### 4. Router — `router/mod.rs`

- Added `window_order: Vec<WindowId>` field for stable cycling order
- `remove_window(&mut self, id: &WindowId)` removes from both `routes` and `window_order`
- `create_window()` returns `WindowId` (was void)

### 5. Application Integration — `application.rs`

Core methods on `impl Application<'_>` (not the `ApplicationHandler` trait impl):

- `align_windows_with(override_focused: Option<WindowId>)` — main layout method; reads config, gets screen area, calls `apply_layout()`
- `align_windows()` — convenience wrapper calling `align_windows_with(None)`
- `cycle_window_focus(reverse: bool)` — finds focused window, calls `cycle_focus()`

Guard: all methods early-return if `auto_align` is false in config.

### 6. Configuration — `rio-backend/src/config/window.rs`

```toml
[window]
auto-align = true       # bool, default false — enables the feature
peek-width = 50         # u32, default 50 — reserved for future use
align-gap = 20          # u32, default 10 — pixels between windows
align-width = 0.8       # f32, default 1.0 — focused window width as ratio of screen (0.1–1.0)
```

### 7. Platform Notes

**macOS:** `get_available_screen_area()` uses `CGDisplay::main()` from the `core-graphics` crate instead of `current_monitor()`. The winit/objc2-foundation `NSScreen` enumeration crashes due to `NSEnumerator` type mismatch. Core Graphics returns logical points directly.

**Cargo dependency:** `core-graphics = "0.24.0"` added under `[target.'cfg(target_os = "macos")'.dependencies]` in `frontends/rioterm/Cargo.toml`.

## Visual Example

```
3 windows, Window B focused (align-width: 0.8, gap: 20):

  |20|        Window B (FOCUSED)        |20|  Window A  |20|
  |  |          1638px wide             |  |  ~370px    |  |
  |  |                                  |  |  ~556px h  |  |
  |  |                                  |  |------------|  |
  |  |          full height             |  |  Window C  |  |
  |  |          minus gaps              |  |  ~370px    |  |
  |  |                                  |  |  ~556px h  |  |

Cycle next → focus C:

  |20|        Window C (FOCUSED)        |20|  Window A  |20|
  |  |          1638px wide             |  |  ~370px    |  |
  |  |                                  |  |------------|  |
  |  |                                  |  |  Window B  |  |
  |  |                                  |  |  ~370px    |  |

Cycle next → focus A:

  |20|        Window A (FOCUSED)        |20|  Window B  |20|
  |  |          1638px wide             |  |  ~370px    |  |
  |  |                                  |  |------------|  |
  |  |                                  |  |  Window C  |  |
  |  |                                  |  |  ~370px    |  |

Single window (centered):

  |        |20|      Window A (FOCUSED)     |20|        |
  |        |  |        1638px wide          |  |        |
  |  205px |  |        centered             |  | 205px  |
```

## Implementation Phases (Completed)

1. Added `router/alignment.rs` with screen geometry and layout calculator
2. Hooked into `WindowEvent::Focused(true)` to trigger layout on focus change
3. Hooked into `create_window()` and window close for layout recalculation
4. Added `AlignWindows`, `CycleWindowNext`, `CycleWindowPrev` keybindings
5. Added config options (`auto-align`, `align-gap`, `align-width`)
6. Fixed macOS screen detection (CGDisplay instead of NSScreen)
7. Fixed logical vs physical coordinate handling for Retina displays
8. Switched from left/right peek to right-side stack layout

## Dependencies

- `winit` / `rio-window`: `Window::set_outer_position()`, `Window::request_inner_size()`, `Window::focus_window()`
- `core-graphics` (macOS only): `CGDisplay::main()` for screen bounds
- `route.window.is_focused` (already exists)
- No external dependencies beyond `core-graphics`
- No IPC needed (all in-process)
