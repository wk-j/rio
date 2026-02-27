# CR-014: Focus Front / Stack Back Window Alignment

**Status:** Proposed
**Date:** 2026-02-27
**Author:** wk

## Summary

A second window alignment mode for Rio terminal: **Focus Front / Stack Back**. The focused window is brought to the **frontmost layer at full (or near-full) screen size**, while all unfocused windows are **arranged left-to-right behind it, equally filling the entire desktop space**. Cycling focus promotes a different window to the front and rearranges the others behind it. This gives a "stage manager" experience — one window dominates the screen, others are tiled behind it for quick switching. In stack mode, the focused window height is independently configurable via `align-height`.

Optionally, unfocused windows can be sent to the **desktop wallpaper layer** (`wallpaper-back = true`), placing them behind all other applications — effectively turning them into live terminal wallpapers. When focus cycles, the newly focused window is promoted from the wallpaper layer back to the normal window level, and the previously focused window is demoted to the wallpaper layer. This is inspired by macOS apps that render content at the `kCGDesktopWindowLevel` (see [wallpaper-play](https://github.com/wk-j/wallpaper-play/issues/6)).

## Motivation

CR-001 introduced side-by-side tiling (focused left, others stacked right). That works well for monitoring multiple terminals simultaneously but sacrifices screen real estate for the primary window. Many users prefer a **maximized primary window** workflow where the active terminal uses nearly the full screen and secondary windows stay available but out of the way — similar to macOS Stage Manager. This mode keeps the focused window as large as possible while still providing visual cues that other windows exist behind it. When focus switches, **all windows resize to fit their new positions** — the newly focused window expands to fill the front slot, and the previously focused window shrinks to fill its share of the back row.

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
- Unfocused windows are arranged left-to-right behind the focused window, equally filling the desktop
- All windows resize when focus switches — the new focus expands, the old focus shrinks into the back row

New `CycleStackWindowNext` / `CycleStackWindowPrev` actions and events are added for cycling focus in the stack layout, bound to `Ctrl+Shift+>` / `Ctrl+Shift+<`. The existing CR-001 side-layout bindings (`Cmd+Shift+>` / `Alt+Shift+>`) remain unchanged.

## Layout Behavior

### Focus Front + Back Row

The focused window is centered at a large size and brought to the front (topmost z-order). All unfocused windows are arranged **left-to-right in a row behind it**, each equally sized to fill the entire desktop width. When focus switches, every window resizes to fit its new position.

```
     Desktop visible area (what the user sees)
|<------------------------------------------->|
|                                              |
|  +--- Win B ---+--- Win C ---+  (behind)     |
|  |             |             |               |
|  +--+--- Window A (FOCUSED) ---+--+          |
|     |                           |            |
|     |    FOCUSED WINDOW         |            |
|     |    (nearly full screen)   |            |
|     |    centered, topmost      |            |
|     |                           |            |
|     +---------------------------+            |
|  |             |             |               |
|  +-------------+-------------+  (behind)     |
|                                              |
```

The unfocused windows fill the full screen space from left to right, split equally in width and using the full screen height. They sit behind the focused window in z-order.

When `wallpaper-back = true`, unfocused windows are sent to the **macOS desktop wallpaper layer** (`kCGDesktopWindowLevel + 1`), placing them behind all normal application windows — they become live terminal wallpapers visible on the desktop background. The focused window stays at the normal window level. When focus cycles, the newly focused window is promoted back to normal level and the previously focused window is demoted to the wallpaper layer.

### Layout Rules

| Window Count | Focused Window | Unfocused Windows |
|---|---|---|
| 1 | No alignment (stays at user's position/size) | none |
| 2 | Front, centered, `align-width` × `align-height` ratio | 1 behind, full screen size |
| 3 | Front, centered, `align-width` × `align-height` ratio | 2 behind, each 50% screen width, left-to-right |
| N | Front, centered, `align-width` × `align-height` ratio | N-1 behind, each `1/(N-1)` screen width, left-to-right |

Positioning details:
- **Single window:** no automatic alignment — window stays at user-defined position and size
- **Focused (2+ windows):** centered on screen at `align-width` ratio. In stack mode, height is controlled by `align-height` ratio. Brought to front (topmost z-order)
- **Back row:** unfocused windows are arranged left-to-right, each getting `screen_width / (N-1)` width and full screen height (minus gap and decoration). They are positioned behind the focused window in z-order.
- **Z-order:** the focused window is always topmost; back-row windows are positioned first so the OS stacks them behind the focused window. When `wallpaper-back = true`, back-row windows are set to `kCGDesktopWindowLevel + 1` (wallpaper layer) and the focused window is restored to normal window level
- **Resize on switch:** when focus changes, **all windows resize** — the new focus expands to fill the front slot and is promoted to normal window level, the previous focus shrinks to fill its share of the back row and is demoted to the wallpaper layer (if `wallpaper-back` is enabled)

### Focus Cycling (Carousel)

The same ring-based cycling as CR-001. Cycling promotes the next window to the front and rearranges all others into the back row.

Example with [A, B, C], focus A:
```
front: A (large, centered)
back row: [B (left half), C (right half)]
```
Cycle next → focus B:
```
front: B (large, centered)
back row: [C (left half), A (right half)]
```
Cycle next → focus C:
```
front: C (large, centered)
back row: [A (left half), B (right half)]
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
- Config reload — re-applies layout with updated settings, resets active mode to config value
- Manual trigger (`AlignWindows` action)
- Focus cycling — side layout (`CycleWindowNext` / `CycleWindowPrev` actions) — sets active mode to Side
- Focus cycling — stack layout (`CycleStackWindowNext` / `CycleStackWindowPrev` actions) — sets active mode to Stack

### Active Align Mode Persistence

The app remembers the **last-used layout mode** at runtime via an `active_align_mode` field on `Application`. This is initialized from the `align-mode` config value on startup and updated whenever the user cycles windows:

- `CycleWindowNext` / `CycleWindowPrev` → sets active mode to **Side**
- `CycleStackWindowNext` / `CycleStackWindowPrev` → sets active mode to **Stack**

Subsequent automatic alignment events (new window creation, focus change, `AlignWindows` action) use the **active mode**, not the config value. This means if the user presses `Ctrl+Shift+.` to switch to stack mode, then opens a new window, the new window will be aligned using stack layout — not falling back to the config default.

Config reload resets the active mode to the config value.

### Keyboard-Only Focus Mode

Same behavior as CR-001: when `keyboard-only-focus = true`, mouse clicks on back-row windows give OS focus but don't trigger layout recalculation.

## Implementation

### 1. Configuration — `rio-backend/src/config/window.rs`

Add a new `align-mode` option:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlignMode {
    /// CR-001: focused left, others stacked right (default)
    Side,
    /// CR-014: focused front, others arranged left-to-right behind
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

/// Focused window height as a ratio of available screen height
/// (0.1–1.0). Only affects the focused window in stack layout.
/// Side layout and unfocused windows always use full screen height.
/// Default: 1.0 (full height).
#[serde(default = "default_align_height", rename = "align-height")]
pub align_height: f32,
```

```rust
/// When true, unfocused windows in stack mode are sent to the
/// macOS desktop wallpaper layer (kCGDesktopWindowLevel + 1),
/// placing them behind all normal applications. The focused
/// window stays at the normal window level. On non-macOS
/// platforms this option is ignored.
/// Default: false.
#[serde(default = "bool::default", rename = "wallpaper-back")]
pub wallpaper_back: bool,
```

With defaults:

```rust
fn default_align_height() -> f32 {
    1.0
}
```

Full TOML example:

```toml
[window]
auto-align = true
align-mode = "stack"       # "side" (CR-001 default) or "stack" (this CR)
align-width = 0.9          # focused window width as ratio of screen (0.1–1.0)
align-height = 0.9         # focused window height in stack mode (0.1–1.0, stack layout only)
align-gap = 20             # pixels of margin around the focused window
keyboard-only-focus = true
wallpaper-back = false     # send unfocused windows to desktop wallpaper layer (macOS only, stack mode)
```

### 2. Layout Engine — `router/alignment.rs`

Add a new `apply_stack_layout()` function alongside the existing `apply_layout()`:

```rust
/// Apply focus-front / back-row layout.
///
/// The focused window is centered on screen at `align_width` ratio
/// (width) and `align_height` ratio (height), brought to the front.
/// Unfocused windows are arranged left-to-right behind it, equally
/// filling the full desktop width and height. All windows resize
/// when focus switches.
pub fn apply_stack_layout(
    routes: &mut FxHashMap<WindowId, Route>,
    focused_id: WindowId,
    window_order: &[WindowId],
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    align_height: f32,
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
    let full_h = screen
        .height
        .saturating_sub(gap * 2 + decoration_height);
    let h_ratio = align_height.clamp(0.1, 1.0);
    let h = (full_h as f32 * h_ratio) as u32;
    let base_x = screen.x
        + ((screen.width.saturating_sub(w)) / 2) as i32;
    let base_y = screen.y + gap as i32;

    // Collect unfocused windows in ring order
    let focused_idx = window_order
        .iter()
        .position(|id| *id == focused_id)
        .unwrap_or(0);

    let mut back_windows: Vec<WindowId> =
        Vec::with_capacity(len - 1);
    for step in 1..len {
        let idx = (focused_idx + step) % len;
        back_windows.push(window_order[idx]);
    }

    // Back-row: arrange unfocused windows left-to-right,
    // each getting equal share of the full screen width.
    // Positioned FIRST so that the focused window (last)
    // ends up on top in z-order.
    let back_count = back_windows.len() as u32;
    let total_back_gaps = (back_count.saturating_sub(1)) * gap;
    let available_back_width =
        screen.width.saturating_sub(gap * 2 + total_back_gaps);
    let back_w = available_back_width / back_count;
    let back_h = h; // same height as focused window

    for (i, id) in back_windows.iter().enumerate().rev() {
        let x = screen.x
            + gap as i32
            + (i as u32 * (back_w + gap)) as i32;
        let slot = WindowSlot {
            x,
            y: base_y,
            width: back_w,
            height: back_h,
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
/// All windows resize to fit their new positions.
pub fn cycle_focus_stack(
    routes: &mut FxHashMap<WindowId, Route>,
    window_order: &[WindowId],
    current_focused: WindowId,
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    align_height: f32,
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
        align_height,
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

The `Application` struct tracks the **active align mode** at runtime via an `active_align_mode` field, initialized from config and updated when the user cycles windows. `align_windows_with()` dispatches based on `self.active_align_mode` (not `self.config.window.align_mode`), so new windows and automatic alignment events use the last-selected layout mode. Config reload resets it.

```rust
pub struct Application<'a> {
    // ...existing fields...

    /// The active alignment mode, updated by cycle shortcuts.
    /// Used by align_windows_with() so new windows use the
    /// last-used layout mode.
    active_align_mode: AlignMode,
}

fn align_windows_with(
    &mut self,
    override_focused: Option<WindowId>,
) {
    // ...setup omitted for brevity...

    match self.active_align_mode {
        AlignMode::Side => {
            crate::router::alignment::apply_layout(...);
        }
        AlignMode::Stack => {
            crate::router::alignment::apply_stack_layout(...);
        }
    }
}

/// Cycle using side layout. Sets active mode to Side.
fn cycle_window_focus(&mut self, reverse: bool) {
    // ...setup omitted...
    self.keyboard_triggered_focus = true;
    self.active_align_mode = AlignMode::Side;
    // ...call cycle_focus()...
}

/// Cycle using stack layout. Sets active mode to Stack.
fn cycle_stack_window_focus(&mut self, reverse: bool) {
    // ...setup omitted...
    self.keyboard_triggered_focus = true;
    self.active_align_mode = AlignMode::Stack;
    // ...call cycle_focus_stack()...
}
```

On config reload, reset to config value:

```rust
self.config = config;
self.active_align_mode = self.config.window.align_mode;
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

**macOS z-order:** `focus_window()` via winit brings a window to the front on macOS. The back-row windows are positioned first (in reverse order) so that the OS window stacking order places them behind the focused window. The focused window is positioned last and `focus_window()` is called on it, ensuring it is the topmost window.

**Window ordering guarantee:** On macOS, calling `set_outer_position()` on a window may bring it forward. To ensure correct z-order, back-row windows are positioned in reverse order, and the focused window is positioned and focused last. If this proves insufficient, we may need to use `NSWindow::orderWindow:relativeTo:` via raw platform access.

**Resize on focus switch:** All windows resize when focus changes. The newly focused window expands to `align-width` ratio (and `align-height` ratio in stack mode), the previously focused window shrinks to fit its equal share of the back row. This ensures every window always fits its assigned position.

**Same `core-graphics` dependency** as CR-001 for macOS screen detection.

### 10. Wallpaper Back Layer — `router/alignment.rs` + platform code (macOS only)

When `wallpaper-back = true` and the active align mode is `Stack`, unfocused windows are demoted to the macOS **desktop wallpaper layer** so they appear behind all normal application windows, acting as live terminal wallpapers.

#### macOS Window Levels

macOS organizes windows into layers by `NSWindowLevel` (backed by `CGWindowLevel`). Key levels from lowest to highest:

| Level | CGWindowLevelForKey constant | Value (approx) | Description |
|---|---|---|---|
| Desktop | `kCGDesktopWindowLevelKey` | -2147483623 | Desktop icons (Finder) |
| Desktop + 1 | `kCGDesktopWindowLevelKey + 1` | -2147483622 | **Wallpaper apps** — above desktop, below everything else |
| Normal | `kCGNormalWindowLevelKey` | 0 | Standard application windows |
| Floating | `kCGFloatingWindowLevelKey` | 3 | Always-on-top panels |

By setting an `NSWindow`'s level to `kCGDesktopWindowLevel + 1`, the window sits just above the desktop background but below all normal app windows — it becomes a "live wallpaper".

#### How It Works

```
┌─────────────────────────────────────────────────┐
│  Normal Window Level (0)                        │
│  ┌───────────────────────────────────────────┐  │
│  │  Focused Rio Terminal (front, interactive) │  │
│  │  Normal NSWindowLevel — receives input      │  │
│  └───────────────────────────────────────────┘  │
│                                                 │
│  ── other apps are here (browsers, editors) ──  │
│                                                 │
├─────────────────────────────────────────────────┤
│  Desktop + 1 Level (wallpaper layer)            │
│  ┌──── Win B ────┬──── Win C ────┐              │
│  │  unfocused    │  unfocused    │              │
│  │  terminal     │  terminal     │              │
│  │  (wallpaper)  │  (wallpaper)  │              │
│  └───────────────┴───────────────┘              │
├─────────────────────────────────────────────────┤
│  Desktop Level                                  │
│  ┌───────────────────────────────────────────┐  │
│  │  Finder desktop icons / actual wallpaper   │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

#### Implementation Detail

The window level manipulation uses the cross-platform `WindowLevel` enum from `rio-window`. A new `Desktop` variant was added to the existing enum:

```rust
// rio-window/src/window.rs
pub enum WindowLevel {
    /// Sits just above the desktop background, below all normal
    /// windows. On macOS maps to kCGDesktopWindowLevel + 1.
    Desktop,
    AlwaysOnBottom,
    #[default]
    Normal,
    AlwaysOnTop,
}
```

The macOS window delegate maps `Desktop` to the wallpaper layer:

```rust
// rio-window/src/platform_impl/macos/window_delegate.rs
WindowLevel::Desktop => {
    (ffi::kCGDesktopWindowLevel + 1) as NSWindowLevel
}
```

In `apply_stack_layout()`, after positioning windows:

```rust
let back_level = if wallpaper_back {
    WindowLevel::Desktop
} else {
    WindowLevel::Normal
};

// Set unfocused back-row windows to wallpaper layer (or normal)
for (i, id) in back_windows.iter().enumerate().rev() {
    if let Some(route) = routes.get_mut(id) {
        apply_slot(route, &slot);
        route.window.winit_window.set_window_level(back_level);
    }
}

// Focused window always at normal level
if let Some(route) = routes.get_mut(&focused_id) {
    apply_slot(route, &focused_slot);
    route.window.winit_window.set_window_level(WindowLevel::Normal);
    route.window.winit_window.focus_window();
}
```

On config reload, all windows are first restored to `WindowLevel::Normal` before re-alignment, ensuring that windows are brought back from the wallpaper layer when `wallpaper-back` is disabled or the mode changes.

#### Window Level Transitions on Focus Cycle

When focus cycles from window A to window B:

1. **Window A** (previously focused): resized to back-row slot, level set to `kCGDesktopWindowLevel + 1` (wallpaper layer)
2. **Window B** (newly focused): resized to focused slot, level set to `kCGNormalWindowLevel` (normal), `focus_window()` called
3. **Other back-row windows**: remain at wallpaper level, repositioned

#### Behavior Notes

- **Non-macOS platforms:** `wallpaper-back` is ignored — windows stay at normal level with standard z-ordering
- **Disabling:** Setting `wallpaper-back = false` (or omitting it) restores all windows to normal level and uses the default z-order stacking behavior
- **Config reload:** When `wallpaper-back` changes from `true` to `false`, all windows are restored to normal window level
- **Interaction:** Windows at the wallpaper level cannot receive mouse clicks or keyboard input through normal means — only the focused window (at normal level) is interactive. Cycling focus via keyboard shortcuts (`Ctrl+Shift+>/<`) is required
- **Show Desktop:** macOS "Show Desktop" (F11) hides normal-level windows, revealing the wallpaper-level terminal windows — they become fully visible as if they were the desktop background
- **Spaces:** Wallpaper-level windows should use `NSWindowCollectionBehavior::CanJoinAllSpaces | Stationary` to appear on all virtual desktops, or `MoveToActiveSpace` to follow the user (configurable)

## Visual Examples

### 3 windows, Window A focused (align-width: 0.9, align-height: 1.0, gap: 20):

```
  +----------------------------------------------------+
  |gap                                              gap|
  |  +-- Win C --+gap+-- Win B --+  (back row, behind) |
  |  |           |   |           |                      |
  |  |  +---- Window A (FOCUSED) -----+                 |
  |  |  |                             |                 |
  |  |  |     FOCUSED WINDOW          |                 |
  |  |  |     90% screen width        |                 |
  |  |  |     centered, topmost       |                 |
  |  |  |                             |                 |
  |  |  +-----------------------------+                 |
  |  |           |   |           |                      |
  |  +-----------+   +-----------+  (back row, behind)  |
  |                                                     |
```

Back row: Win C (left 50%) and Win B (right 50%), full screen height, behind focused.

### Cycle next → focus B:

```
  +----------------------------------------------------+
  |  +-- Win A --+gap+-- Win C --+  (back row, behind) |
  |  |           |   |           |                      |
  |  |  +---- Window B (FOCUSED) -----+                 |
  |  |  |                             |                 |
  |  |  |     FOCUSED WINDOW          |                 |
  |  |  |     90% screen width        |                 |
  |  |  |                             |                 |
  |  |  +-----------------------------+                 |
  |  |           |   |           |                      |
  |  +-----------+   +-----------+  (back row, behind)  |
```

Window A shrinks from focused size to back-row size. Window B expands to focused size.

### Cycle next → focus C:

```
  +----------------------------------------------------+
  |  +-- Win B --+gap+-- Win A --+  (back row, behind) |
  |  |           |   |           |                      |
  |  |  +---- Window C (FOCUSED) -----+                 |
  |  |  |                             |                 |
  |  |  |     FOCUSED WINDOW          |                 |
  |  |  |     90% screen width        |                 |
  |  |  |                             |                 |
  |  |  +-----------------------------+                 |
  |  |           |   |           |                      |
  |  +-----------+   +-----------+  (back row, behind)  |
```

### Single window:

```
  Window stays at user's position/size — no automatic alignment.
```

## Implementation Phases

1. Add `AlignMode` enum and `align_mode` field to `rio-backend/src/config/window.rs`
2. Add `CycleStackWindowNext` / `CycleStackWindowPrev` to `RioEvent` enum in `rio-backend/src/event/mod.rs`
3. Add `CycleStackWindowNext` / `CycleStackWindowPrev` to `Action` enum + string parsing in `frontends/rioterm/src/bindings/mod.rs`
4. Add default keybindings `Ctrl+Shift+.` / `Ctrl+Shift+,` to all platform binding blocks
5. Add `apply_stack_layout()` and `cycle_focus_stack()` to `router/alignment.rs` — unfocused windows arranged left-to-right filling desktop
6. Add `cycle_stack_window_next()` / `cycle_stack_window_prev()` to `ContextManager`
7. Add `Act::CycleStackWindowNext` / `Act::CycleStackWindowPrev` dispatch in `Screen`
8. Add `cycle_stack_window_focus()` to `Application` and handle the new events in `user_event()`
9. Modify `align_windows_with()` to dispatch based on `align_mode` config
10. Remove `stack_offset` config field (no longer needed — back row fills desktop evenly)
11. Test z-order behavior on macOS — ensure focused window is always topmost
12. Test with 2, 3, 4+ windows to verify all windows resize correctly on focus switch
13. Test that `Ctrl+Shift+>` / `<` always uses stack layout, `Cmd+Shift+>` / `<` always uses side layout
14. If z-order is unreliable via winit, add platform-specific `NSWindow::orderWindow:relativeTo:` fallback
15. Add `wallpaper_back` config field and default to `rio-backend/src/config/window.rs`
16. Add `Desktop` variant to `WindowLevel` enum in `rio-window/src/window.rs`, map to `kCGDesktopWindowLevel + 1` in macOS delegate
17. Add window level transitions in `apply_stack_layout()` — demote unfocused to `WindowLevel::Desktop`, promote focused to `WindowLevel::Normal`
18. Handle config reload: restore all windows to normal level when `wallpaper-back` changes from `true` to `false`
19. Test wallpaper layer behavior: verify unfocused windows sit behind all apps, verify "Show Desktop" (F11) reveals them, verify focus cycling promotes/demotes correctly

## File Changes

| File | Change |
|---|---|
| `rio-backend/src/config/window.rs` | Add `AlignMode` enum, `align_mode` field, `align_height` field, `wallpaper_back` field, defaults. Remove `stack_offset` field |
| `rio-backend/src/event/mod.rs` | Add `CycleStackWindowNext`, `CycleStackWindowPrev` variants to `RioEvent` |
| `frontends/rioterm/src/bindings/mod.rs` | Add `CycleStackWindowNext`, `CycleStackWindowPrev` to `Action` enum, string parsing, and default keybindings (`Ctrl+Shift+.` / `Ctrl+Shift+,`) |
| `frontends/rioterm/src/router/alignment.rs` | Add `apply_stack_layout()` (back-row left-to-right), `cycle_focus_stack()`. Add `align_height` parameter to `apply_stack_layout()` and `cycle_focus_stack()` (stack layout only). Add `wallpaper_back` parameter for window level manipulation (macOS) |
| `frontends/rioterm/src/context/mod.rs` | Add `cycle_stack_window_next()`, `cycle_stack_window_prev()` methods |
| `frontends/rioterm/src/screen/mod.rs` | Add dispatch for `Act::CycleStackWindowNext`, `Act::CycleStackWindowPrev` |
| `frontends/rioterm/src/application.rs` | Add `cycle_stack_window_focus()`, handle new events, modify `align_windows_with()` to match on `align_mode`. Remove `stack_offset` usage. Restore all windows to `Normal` level on config reload before re-alignment |
| `rio-window/src/window.rs` | Add `Desktop` variant to `WindowLevel` enum |
| `rio-window/src/platform_impl/macos/window_delegate.rs` | Map `WindowLevel::Desktop` to `(kCGDesktopWindowLevel + 1) as NSWindowLevel` |
| `rio-window/src/platform_impl/orbital/window.rs` | Handle `WindowLevel::Desktop` (treated as `AlwaysOnBottom`) |

## Dependencies

- Same as CR-001: `winit`/`rio-window`, `core-graphics` (macOS)
- `wallpaper-back` feature uses `kCGDesktopWindowLevel` constant from `rio-window/src/platform_impl/macos/ffi.rs` (already defined) and `NSWindow::setLevel` access via `rio-window` platform APIs
- No new external crate dependencies
