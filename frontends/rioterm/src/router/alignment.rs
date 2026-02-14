use rio_backend::event::WindowId;
use rio_window::dpi::{LogicalPosition, LogicalSize};
use rustc_hash::FxHashMap;

use super::Route;

/// Represents the usable screen area (accounting for menu bar, dock, taskbar).
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

/// Which screen edge an unfocused window peeks from.
#[derive(Debug, Clone, Copy)]
pub enum PeekSide {
    Left,
    Right,
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
        // On non-macOS, try current_monitor first
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
        // Fallback: use the window's outer size positioned at origin
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

/// Position for the focused window: centered with `align_width` ratio and gap.
pub fn focused_slot(screen: &ScreenArea, gap: u32, align_width: f32) -> WindowSlot {
    let ratio = align_width.clamp(0.1, 1.0);
    let w = (screen.width as f32 * ratio) as u32;
    let h = screen.height.saturating_sub(gap * 2);
    let x = screen.x + ((screen.width - w) / 2) as i32;
    let y = screen.y + gap as i32;
    WindowSlot {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Position for an unfocused window peeking from an edge.
///
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
    let count = stack_count.max(1) as u32;
    let slot_height = screen.height / count;
    let y = screen.y + (stack_index as u32 * slot_height) as i32;

    WindowSlot {
        x,
        y,
        width: screen.width,
        height: slot_height,
    }
}

/// Apply a computed slot (position + size) to a window using logical coordinates.
///
/// On macOS, `CGDisplay` bounds are in points (logical), so we use
/// `LogicalPosition` / `LogicalSize` to avoid double scale-factor division.
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

/// Apply focus-centered layout to all windows.
///
/// The focused window fills the screen center. Unfocused windows are
/// distributed to left/right peek positions (alternating, stacked
/// vertically when multiple windows share a side).
pub fn apply_layout(
    routes: &mut FxHashMap<WindowId, Route>,
    focused_id: WindowId,
    window_order: &[WindowId],
    screen: &ScreenArea,
    peek_width: u32,
    gap: u32,
    align_width: f32,
) {
    // Move focused window to center
    if let Some(route) = routes.get_mut(&focused_id) {
        let slot = focused_slot(screen, gap, align_width);
        apply_slot(route, &slot);
    }

    // Distribute unfocused windows to left/right peek positions
    let mut left_windows: Vec<WindowId> = Vec::new();
    let mut right_windows: Vec<WindowId> = Vec::new();

    for id in window_order {
        if *id == focused_id {
            continue;
        }
        // Alternate: fill the side with fewer windows first
        if left_windows.len() <= right_windows.len() {
            left_windows.push(*id);
        } else {
            right_windows.push(*id);
        }
    }

    // Position left-side peek windows (stacked vertically)
    for (i, id) in left_windows.iter().enumerate() {
        let slot = peek_slot(screen, PeekSide::Left, peek_width, i, left_windows.len());
        if let Some(route) = routes.get_mut(id) {
            apply_slot(route, &slot);
        }
    }

    // Position right-side peek windows (stacked vertically)
    for (i, id) in right_windows.iter().enumerate() {
        let slot = peek_slot(screen, PeekSide::Right, peek_width, i, right_windows.len());
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

    // Focus the new window (this will also trigger WindowEvent::Focused)
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
