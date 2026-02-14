use rio_backend::event::WindowId;
use rio_window::dpi::{LogicalPosition, LogicalSize};
use rustc_hash::FxHashMap;

use super::Route;

/// Represents the usable screen area.
#[derive(Debug, Clone, Copy)]
pub struct ScreenArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A computed position and size for a window slot.
#[derive(Debug, Clone, Copy)]
pub struct WindowSlot {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Get the available screen area for the main display.
///
/// On macOS, uses `CGDisplay::main()` via Core Graphics to avoid
/// the NSScreen enumeration crash in objc2-foundation.
/// On other platforms, uses `current_monitor()` with a fallback.
pub fn get_available_screen_area(
    _window: &rio_window::window::Window,
) -> Option<ScreenArea> {
    #[cfg(target_os = "macos")]
    {
        use core_graphics::display::CGDisplay;
        let main = CGDisplay::main();
        let bounds = main.bounds();
        let width = bounds.size.width as u32;
        let height = bounds.size.height as u32;
        if width == 0 || height == 0 {
            return None;
        }
        Some(ScreenArea {
            x: bounds.origin.x as i32,
            y: bounds.origin.y as i32,
            width,
            height,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(monitor) = _window.current_monitor() {
            let size = monitor.size();
            let pos = monitor.position();
            return Some(ScreenArea {
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
            });
        }
        let size = _window.outer_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some(ScreenArea {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        })
    }
}

/// Position for the focused window.
///
/// - With 1 window: centered at `align_width` ratio.
/// - With 2 windows: left-aligned at `align_width` ratio.
/// - With 3+ windows: left-aligned at `align_width` ratio.
pub fn focused_slot(
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    has_peers: bool,
) -> WindowSlot {
    let ratio = align_width.clamp(0.1, 1.0);
    let w = (screen.width as f32 * ratio) as u32;
    let h = screen.height.saturating_sub(gap * 2);
    let x = if has_peers {
        // Left-aligned with gap
        screen.x + gap as i32
    } else {
        // Centered
        screen.x + ((screen.width - w) / 2) as i32
    };
    let y = screen.y + gap as i32;
    WindowSlot {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Apply a computed slot (position + size) to a window using logical coordinates.
fn apply_slot(route: &mut Route, slot: &WindowSlot) {
    route
        .window
        .winit_window
        .set_outer_position(LogicalPosition::new(slot.x, slot.y));
    let _ = route
        .window
        .winit_window
        .request_inner_size(LogicalSize::new(slot.width, slot.height));
}

/// Apply focus-centered layout with right-side stack.
///
/// The focused window sits on the left at `align_width` ratio.
/// All unfocused windows are stacked vertically on the right side,
/// sharing the remaining screen width equally in height.
///
/// Cycling rotates which window is focused — the focused window
/// always moves to the left, others stack on the right.
///
/// Example with [A, B, C], focus B:
///   left: B (80%)  right stack: [A, C] (20%, split vertically)
/// Cycle next, focus C:
///   left: C (80%)  right stack: [A, B] (20%, split vertically)
pub fn apply_layout(
    routes: &mut FxHashMap<WindowId, Route>,
    focused_id: WindowId,
    window_order: &[WindowId],
    screen: &ScreenArea,
    _peek_width: u32,
    gap: u32,
    align_width: f32,
) {
    let len = window_order.len();
    if len == 0 {
        return;
    }

    let has_peers = len > 1;

    // Position focused window (centered if alone, left-aligned if has peers)
    let focused = focused_slot(screen, gap, align_width, has_peers);
    if let Some(route) = routes.get_mut(&focused_id) {
        apply_slot(route, &focused);
    }

    if !has_peers {
        return;
    }

    // Collect unfocused windows in ring order (preserves carousel rotation)
    let focused_idx = window_order
        .iter()
        .position(|id| *id == focused_id)
        .unwrap_or(0);

    let mut stack_windows: Vec<WindowId> = Vec::with_capacity(len - 1);
    for step in 1..len {
        let idx = (focused_idx + step) % len;
        stack_windows.push(window_order[idx]);
    }

    // Stack area: right of focused window + gap, filling to screen edge
    let stack_x = focused.x + focused.width as i32 + gap as i32;
    let screen_right = screen.x + screen.width as i32 - gap as i32;
    let stack_w = (screen_right - stack_x).max(0) as u32;
    let stack_count = stack_windows.len() as u32;

    // Divide height evenly among stacked windows, with gap between them
    let total_gaps = (stack_count.saturating_sub(1)) * gap;
    let available_height = screen.height.saturating_sub(gap * 2 + total_gaps);
    let slot_height = available_height / stack_count;

    for (i, id) in stack_windows.iter().enumerate() {
        let y = screen.y + gap as i32 + (i as u32 * (slot_height + gap)) as i32;
        let slot = WindowSlot {
            x: stack_x,
            y,
            width: stack_w,
            height: slot_height,
        };
        if let Some(route) = routes.get_mut(id) {
            apply_slot(route, &slot);
        }
    }
}

/// Cycle focus to the next or previous window in order.
///
/// Returns the `WindowId` of the newly focused window, or `None` if
/// there are fewer than 2 windows.
pub fn cycle_focus(
    routes: &mut FxHashMap<WindowId, Route>,
    window_order: &[WindowId],
    current_focused: WindowId,
    screen: &ScreenArea,
    peek_width: u32,
    gap: u32,
    align_width: f32,
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

    // Focus the new window
    if let Some(route) = routes.get(&new_focused) {
        route.window.winit_window.focus_window();
    }

    apply_layout(
        routes,
        new_focused,
        window_order,
        screen,
        peek_width,
        gap,
        align_width,
    );
    Some(new_focused)
}
