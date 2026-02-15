# CR-003: Fix Third Window Overlapping Second in Stacked Layout

**Status:** Implemented
**Date:** 2026-02-15
**Author:** wk

## Summary

Fix the auto-alignment layout so stacked (unfocused) windows no longer overlap each other. With 3+ windows, the third window visually overlaps the second because the stacking math does not account for window decoration height (title bar). Two additional contributing factors worsen the issue: macOS screen area includes the menu bar and dock, and the focused window width is not reduced by gap when peers exist.

## Motivation

CR-001 introduced the focus-centered tiling layout. With 2 windows the layout looks correct, but with 3+ windows the stacked windows on the right side overlap each other. The third window's top edge sits underneath the second window's bottom edge instead of below its visible boundary. This defeats the purpose of automatic alignment — users see overlapping content and cannot distinguish the stacked windows.

## Root Cause Analysis

Three issues combine to produce the overlap:

### Problem 1 (PRIMARY): `set_outer_position` + `request_inner_size` mismatch

`apply_slot()` positions windows using **outer** coordinates (includes title bar) but sizes them using **inner** dimensions (content area only). The stacking Y-position assumed each window's total outer height equals `slot_height`, but the actual outer height is `slot_height + title_bar_height` (~28px on macOS). Each successive stacked window overlaps the previous one by the title bar height.

### Problem 2: macOS uses full display bounds

`CGDisplay::main().bounds()` returns the full display resolution including the menu bar (~25px). Windows positioned at `y=0` sit behind the menu bar, and the available height is overestimated.

### Problem 3: Focused window width ignores gap

The focused window width was calculated as `screen.width * ratio` without deducting gaps, consuming too much horizontal space when peers exist.

## Implementation

All changes are in `frontends/rioterm/src/router/alignment.rs`.

### Fix 1: Decoration-aware stacking

`apply_layout()` now queries window decoration height by comparing `outer_size()` vs `inner_size()` (converted from physical to logical using the scale factor), then accounts for it in three places:

1. **Focused window height** — subtracts decoration height so the outer window fits within the screen area
2. **Available stacked height** — reserves `decoration_height * stack_count` in the height budget
3. **Stacking Y-position** — advances each window by `decoration_height + slot_height + gap` instead of just `slot_height + gap`

```rust
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
```

The decoration height is measured once from any existing window (all windows share the same decoration style) and applied consistently to both the focused window and stacked windows.

### Fix 2: Menu bar offset on macOS

`get_available_screen_area()` now subtracts a 25pt menu bar offset from the top of the screen on macOS:

```rust
let menu_bar_height: u32 = 25;
Some(ScreenArea {
    x: bounds.origin.x as i32,
    y: bounds.origin.y as i32 + menu_bar_height as i32,
    width,
    height: height.saturating_sub(menu_bar_height),
})
```

Note: `CGDisplayUsableRect` does not exist as a public symbol in the CoreGraphics framework. `NSScreen.visibleFrame` would be ideal but requires NSScreen access which is unavailable from this crate without adding objc2 dependencies. The fixed 25pt offset handles the standard menu bar; notch displays (37pt) may have slight overlap at the top but the dock is typically at the bottom and not affected.

### Fix 3: Gap-aware focused window width

`focused_slot()` now deducts `gap * 2` from the usable width before applying the `align_width` ratio when peers exist:

```rust
let usable_width = screen.width.saturating_sub(if has_peers { gap * 2 } else { 0 });
let w = (usable_width as f32 * ratio) as u32;
```

This ensures the focused window doesn't consume space needed for the left gap and the gap between focused and stacked windows.

### Updated `focused_slot` signature

`focused_slot()` now takes a `decoration_height: u32` parameter so the focused window's height also accounts for its own title bar:

```rust
let h = screen.height.saturating_sub(gap * 2 + decoration_height);
```

## Files Changed

| File | Change |
|---|---|
| `frontends/rioterm/src/router/alignment.rs` | Added decoration height detection via `outer_size()` / `inner_size()`. Updated `focused_slot()` to accept and use `decoration_height`, deduct gaps from width when peers exist. Updated `apply_layout()` to reserve decoration height in stacking math and advance Y by `decoration_height + slot_height + gap`. Added 25pt menu bar offset on macOS. |

## Dependencies

- No new crate dependencies
- No configuration changes
- Uses existing `outer_size()`, `inner_size()`, and `scale_factor()` from rio-window's `Window` type
