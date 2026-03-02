use rio_backend::event::WindowId;
use rio_window::dpi::{LogicalPosition, LogicalSize};
use rio_window::window::WindowLevel;
use rustc_hash::FxHashMap;

use super::Route;

/// Actual menu bar height in logical points, detected at runtime.
///
/// On macOS this is computed by comparing `NSScreen.frame` (full
/// display) with `NSScreen.visibleFrame` (usable area excluding
/// the menu bar and the dock). When the menu bar is set to
/// auto-hide, `visibleFrame` returns the full height so this
/// value is 0 — no gap is wasted.
///
/// `apply_stack_layout` uses this to reverse the offset for
/// wallpaper-back windows that cover the full display.
#[cfg(target_os = "macos")]
fn get_menu_bar_height() -> u32 {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let ns_screen_class = Class::get("NSScreen").expect("NSScreen class not found");
        let main_screen: *const Object = msg_send![ns_screen_class, mainScreen];
        if main_screen.is_null() {
            return 25; // fallback
        }
        let frame: core_graphics::geometry::CGRect = msg_send![main_screen, frame];
        let visible: core_graphics::geometry::CGRect =
            msg_send![main_screen, visibleFrame];
        // The menu bar sits at the top. In Cocoa coordinates (origin
        // at bottom-left), the menu bar height is the difference
        // between the full frame top and the visible frame top.
        let frame_top = frame.origin.y + frame.size.height;
        let visible_top = visible.origin.y + visible.size.height;
        let menu_bar = (frame_top - visible_top).max(0.0) as u32;
        menu_bar
    }
}

#[cfg(not(target_os = "macos"))]
fn get_menu_bar_height() -> u32 {
    0
}

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
/// On macOS, uses `CGDisplay::main()` via Core Graphics for display
/// bounds, then queries `NSScreen.visibleFrame` via the `objc` crate
/// to get the actual menu bar height. This correctly handles
/// auto-hidden menu bars (height = 0) and notch displays (37pt).
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
        // Query the actual menu bar height at runtime via
        // NSScreen.visibleFrame so auto-hidden menu bars report 0.
        let menu_bar_height = get_menu_bar_height();
        Some(ScreenArea {
            x: bounds.origin.x as i32,
            y: bounds.origin.y as i32 + menu_bar_height as i32,
            width,
            height: height.saturating_sub(menu_bar_height),
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

/// Position for the focused window (side layout).
///
/// - With 1 window: no alignment (handled by caller returning early).
/// - With 2+ windows: left-aligned at `align_width` ratio.
///
/// Height always fills the available screen area (full height).
pub fn focused_slot(
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    has_peers: bool,
    decoration_height: u32,
) -> WindowSlot {
    let ratio = align_width.clamp(0.1, 1.0);
    let usable_width = screen
        .width
        .saturating_sub(if has_peers { gap * 2 } else { 0 });
    let w = (usable_width as f32 * ratio) as u32;
    // Subtract decoration height so the outer window (content + title bar)
    // fits within the screen area.
    let h = screen.height.saturating_sub(gap * 2 + decoration_height);
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
    // Skip alignment for 0 or 1 window - leave single window at user's position/size
    if len < 2 {
        return;
    }

    // Determine window decoration (title bar) height by comparing
    // outer_size vs inner_size on any existing window. This is the
    // height added by the OS window chrome that we must account for
    // when positioning windows so they don't overlap.
    let decoration_height = routes
        .values()
        .next()
        .map(|route| {
            let outer = route.window.winit_window.outer_size();
            let inner = route.window.winit_window.inner_size();
            // Convert physical pixels to logical points using scale factor
            let scale = route.window.winit_window.scale_factor();
            ((outer.height.saturating_sub(inner.height)) as f64 / scale) as u32
        })
        .unwrap_or(0);

    // Position focused window (left-aligned since we have multiple windows)
    let focused = focused_slot(screen, gap, align_width, true, decoration_height);
    if let Some(route) = routes.get_mut(&focused_id) {
        apply_slot(route, &focused);
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

    // Divide height evenly among stacked windows, with gap between them.
    // Each window's outer height = decoration_height + slot_height (inner),
    // so we must reserve space for all decoration heights too.
    let total_gaps = (stack_count.saturating_sub(1)) * gap;
    let total_decorations = stack_count * decoration_height;
    let available_height = screen
        .height
        .saturating_sub(gap * 2 + total_gaps + total_decorations);
    let slot_height = available_height / stack_count;

    for (i, id) in stack_windows.iter().enumerate() {
        // Each window's outer height is (decoration_height + slot_height),
        // so advance Y by that amount plus the gap between windows.
        let y = screen.y
            + gap as i32
            + (i as u32 * (decoration_height + slot_height + gap)) as i32;
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

/// Apply focus-front / back-row layout.
///
/// The focused window is centered on screen at `align_width` ratio
/// and brought to the front. Unfocused windows are arranged
/// left-to-right behind it, equally filling the full desktop
/// width and height. All windows resize when focus switches.
///
/// Example with [A, B, C], focus A:
///   front: A (large, centered)
///   back row: [B (left half), C (right half)]
/// Cycle next, focus B:
///   front: B (large, centered)
///   back row: [C (left half), A (right half)]
pub fn apply_stack_layout(
    routes: &mut FxHashMap<WindowId, Route>,
    focused_id: WindowId,
    window_order: &[WindowId],
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    align_height: f32,
    wallpaper_back: bool,
) {
    let len = window_order.len();
    if len < 2 {
        // When wallpaper_back is active the single remaining window
        // may have been a back-row window at Desktop level and
        // back-row size. Resize it to the focused slot so it looks
        // correct as the only window on screen.
        if wallpaper_back && len == 1 {
            if let Some(route) = routes.get_mut(&focused_id) {
                let decoration_height = {
                    let outer = route.window.winit_window.outer_size();
                    let inner = route.window.winit_window.inner_size();
                    let scale = route.window.winit_window.scale_factor();
                    ((outer.height.saturating_sub(inner.height)) as f64 / scale) as u32
                };
                let ratio = align_width.clamp(0.1, 1.0);
                let w = (screen.width.saturating_sub(gap * 2) as f32 * ratio) as u32;
                let full_h = screen.height.saturating_sub(gap * 2 + decoration_height);
                let h_ratio = align_height.clamp(0.1, 1.0);
                let h = (full_h as f32 * h_ratio) as u32;
                let x = screen.x + ((screen.width.saturating_sub(w)) / 2) as i32;
                let base_y = screen.y + gap as i32;
                let y = base_y + ((full_h.saturating_sub(h)) / 2) as i32;
                apply_slot(
                    route,
                    &WindowSlot {
                        x,
                        y,
                        width: w,
                        height: h,
                    },
                );
                route
                    .window
                    .winit_window
                    .set_window_level(WindowLevel::Normal);
                route.window.winit_window.focus_window();
            }
        }
        return;
    }

    let decoration_height = routes
        .values()
        .next()
        .map(|route| {
            let outer = route.window.winit_window.outer_size();
            let inner = route.window.winit_window.inner_size();
            let scale = route.window.winit_window.scale_factor();
            ((outer.height.saturating_sub(inner.height)) as f64 / scale) as u32
        })
        .unwrap_or(0);

    // Compute focused window slot (centered)
    let ratio = align_width.clamp(0.1, 1.0);
    let w = (screen.width.saturating_sub(gap * 2) as f32 * ratio) as u32;
    let full_h = screen.height.saturating_sub(gap * 2 + decoration_height);
    let h_ratio = align_height.clamp(0.1, 1.0);
    let h = (full_h as f32 * h_ratio) as u32;
    let base_x = screen.x + ((screen.width.saturating_sub(w)) / 2) as i32;
    let base_y = screen.y + gap as i32;
    // Vertically center the focused window within the available area
    let focused_y = base_y + ((full_h.saturating_sub(h)) / 2) as i32;

    // Collect unfocused windows in ring order
    let focused_idx = window_order
        .iter()
        .position(|id| *id == focused_id)
        .unwrap_or(0);

    let mut back_windows: Vec<WindowId> = Vec::with_capacity(len - 1);
    for step in 1..len {
        let idx = (focused_idx + step) % len;
        back_windows.push(window_order[idx]);
    }

    // Back-row: arrange unfocused windows left-to-right,
    // each getting an equal share of the full screen width.
    // Positioned in reverse order (rightmost first) so that
    // the focused window (positioned last) ends up on top.
    //
    // When wallpaper_back is enabled the back-row windows sit on the
    // desktop wallpaper layer, so they expand edge-to-edge without
    // any gap or border — they fill the entire screen area.
    let back_count = back_windows.len() as u32;
    let back_gap = if wallpaper_back { 0 } else { gap };
    let total_back_gaps = back_count.saturating_sub(1) * back_gap;
    let available_back_width =
        screen.width.saturating_sub(back_gap * 2 + total_back_gaps);
    let back_w = available_back_width / back_count;
    // When wallpaper_back is enabled, back-row windows cover the
    // full display including the menu bar area. The `screen` passed
    // in has the menu bar already subtracted, so we reverse that
    // adjustment to get the raw display origin and height.
    let menu_bar = get_menu_bar_height();
    let back_y = if wallpaper_back {
        screen.y - menu_bar as i32
    } else {
        base_y
    };
    let back_h = if wallpaper_back {
        (screen.height + menu_bar).saturating_sub(decoration_height)
    } else {
        full_h
    };

    // Window level for unfocused back-row windows: Desktop (wallpaper
    // layer) when wallpaper_back is enabled, Normal otherwise.
    let back_level = if wallpaper_back {
        WindowLevel::Desktop
    } else {
        WindowLevel::Normal
    };

    for (i, id) in back_windows.iter().enumerate().rev() {
        let x = screen.x + back_gap as i32 + (i as u32 * (back_w + back_gap)) as i32;
        let slot = WindowSlot {
            x,
            y: back_y,
            width: back_w,
            height: back_h,
        };
        if let Some(route) = routes.get_mut(id) {
            apply_slot(route, &slot);
            route.window.winit_window.set_window_level(back_level);
        }
    }

    // Position and raise focused window (last = topmost, vertically centered)
    let focused_slot = WindowSlot {
        x: base_x,
        y: focused_y,
        width: w,
        height: h,
    };
    if let Some(route) = routes.get_mut(&focused_id) {
        apply_slot(route, &focused_slot);
        route
            .window
            .winit_window
            .set_window_level(WindowLevel::Normal);
        route.window.winit_window.focus_window();
    }
}

/// Cycle focus to the next or previous window using stack layout.
/// All windows resize to fit their new positions.
///
/// Returns the `WindowId` of the newly focused window, or `None`
/// if there are fewer than 2 windows.
pub fn cycle_focus_stack(
    routes: &mut FxHashMap<WindowId, Route>,
    window_order: &[WindowId],
    current_focused: WindowId,
    screen: &ScreenArea,
    gap: u32,
    align_width: f32,
    align_height: f32,
    wallpaper_back: bool,
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
        wallpaper_back,
    );

    Some(new_focused)
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
