# CR-008: Cursor Glow Overlay & Batched Overlay Rendering

**Status:** Implemented
**Date:** 2026-02-20
**Author:** wk

## Summary

Implement a cursor glow overlay — a semi-transparent blue rounded quad rendered behind the cursor cell — as the first concrete proof of the overlay system described in CR-007. In addition, fix a critical shared-buffer bug in the overlay rendering pipeline by replacing per-overlay `render_single()` calls with a single batched `render_batch()` call.

## Motivation

1. **Prove the overlay pipeline works**: CR-007 proposes a multi-layer overlay system. A cursor glow is the simplest possible overlay (single quad, no text, no images) and validates the full pipeline: state field → public API → render pass.

2. **Visual cursor tracking**: A soft glow behind the cursor improves cursor discoverability, especially in dense terminal output, split panes, or transparent/background-image configurations where the cursor can blend in.

3. **Fix render correctness**: The existing overlay rendering used separate `render_single()` calls per overlay. All overlays shared the same GPU instance buffer, and `queue.write_buffer()` is a queue operation not ordered relative to encoder render passes. This caused the last `write_buffer` call to clobber all previous ones — meaning only the last overlay quad was visible across all render passes. The cursor glow exposed this bug because it disappeared whenever the progress bar was active.

## Architecture

### Overlay Pipeline

```
Renderer::run()                      Sugarloaf::render()
┌──────────────────────┐            ┌─────────────────────────────────┐
│                      │            │ Main render pass (LoadOp::Clear)│
│ Compute cursor glow: │            │  - bg image (LayerBrush)        │
│   cursor grid pos    │            │  - quads (QuadBrush::render)    │
│   → pixel coords     │            │  - rich text (RichTextBrush)    │
│   → glow Quad        │            │                                 │
│   → set_cursor_glow  │            │ Overlay render pass (LoadOp::Load)│
│      _overlay()      │            │  - ALL overlay quads batched:   │
│                      │            │    cursor_glow → vi_mode →      │
│ Compute other        │            │    visual_bell → progress_bar   │
│ overlays...          │            │  (single render_batch call)     │
└──────────────────────┘            │                                 │
                                    │ Filters (if any)                │
                                    └─────────────────────────────────┘
```

### Cursor Position Calculation

```
Terminal grid:  cursor.state.pos = Pos { row: Line, col: Column }
Pane position:  grid.current_position() → [x, y] (screen-space)
Cell size:      cell_w = dim.width / scale
                cell_h = (dim.height / scale) * line_height
Pixel coords:   cursor_x = pane_pos[0] + col * cell_w
                cursor_y = pane_pos[1] + row * cell_h
Glow bounds:    padding = cell_w * 1.5
                glow_x = cursor_x - padding
                glow_y = cursor_y - padding
                glow_w = cell_w + padding * 2
                glow_h = cell_h + padding * 2
```

### Glow Quad Properties

| Property       | Value                              |
|----------------|------------------------------------|
| Color          | `[0.3, 0.5, 1.0, 0.15]` (blue, 15% alpha) |
| Size           | Cell + 1.5× cell_width padding on each side |
| Border radius  | `glow_w / 2.0` (fully circular)    |
| Visibility     | Only when `cursor.state.is_visible()` returns true |
| Shadow         | None (pure alpha-blended quad)     |

## Implementation Details

### 1. SugarState: New overlay field

```rust
// sugarloaf/src/sugarloaf/state.rs
pub struct SugarState {
    // ... existing fields ...
    pub cursor_glow_overlay: Option<Quad>,
}

impl SugarState {
    pub fn set_cursor_glow_overlay(&mut self, overlay: Option<Quad>) {
        self.cursor_glow_overlay = overlay;
    }
}
```

The field is initialized to `None` and is NOT cleared by `reset()` (same as `vi_mode_overlay`, `visual_bell_overlay`, and `progress_bar`). It persists between frames and is overwritten each frame by the renderer.

### 2. Sugarloaf: Public API

```rust
// sugarloaf/src/sugarloaf.rs
impl Sugarloaf {
    pub fn set_cursor_glow_overlay(&mut self, overlay: Option<Quad>) {
        self.state.set_cursor_glow_overlay(overlay);
    }
}
```

### 3. ContextGrid: Position accessor

```rust
// frontends/rioterm/src/context/grid.rs
impl<T: EventListener> ContextGrid<T> {
    /// Get the screen-space position [x, y] of the current (focused) pane.
    pub fn current_position(&self) -> [f32; 2] {
        // Check quick terminal first
        if let Some(ref qt) = self.quick_terminal {
            if qt.visible && qt.item.val.route_id == self.current {
                return qt.item.position();
            }
        }

        if let Some(item) = self.inner.get(&self.current) {
            item.position()
        } else {
            [0.0, 0.0]
        }
    }
}
```

### 4. Renderer: Cursor glow computation

```rust
// frontends/rioterm/src/renderer/mod.rs (inside Renderer::run)
let cursor_glow = {
    let grid = context_manager.current_grid();
    let ctx = grid.current();
    let cursor = &ctx.renderable_content.cursor;

    if cursor.state.is_visible() {
        let pane_pos = grid.current_position();
        let dim = &ctx.dimension;
        let scale = dim.dimension.scale;

        let cell_w = dim.dimension.width / scale;
        let cell_h = (dim.dimension.height / scale) * dim.line_height;

        let col = *cursor.state.pos.col;
        let row = *cursor.state.pos.row as usize;

        let cursor_x = pane_pos[0] + (col as f32) * cell_w;
        let cursor_y = pane_pos[1] + (row as f32) * cell_h;

        let glow_pad = cell_w * 1.5;
        Some(Quad {
            position: [cursor_x - glow_pad, cursor_y - glow_pad],
            size: [cell_w + glow_pad * 2.0, cell_h + glow_pad * 2.0],
            color: [0.3, 0.5, 1.0, 0.15],
            border_radius: [(cell_w + glow_pad * 2.0) / 2.0; 4],
            ..Quad::default()
        })
    } else {
        None
    }
};
sugarloaf.set_cursor_glow_overlay(cursor_glow);
```

### 5. QuadBrush: Batched overlay rendering (bug fix)

The critical fix: instead of calling `render_single()` per overlay (which clobbers the shared instance buffer), all overlay quads are collected into a `Vec<Quad>` and rendered with a single `render_batch()` call.

```rust
// sugarloaf/src/components/quad/mod.rs
impl QuadBrush {
    /// Render multiple quads in a single instanced draw call.
    /// Safe to use alongside other render passes (single write_buffer call).
    pub fn render_batch<'a>(
        &'a mut self,
        context: &mut Context,
        quads: &[Quad],
        render_pass: &mut wgpu::RenderPass<'a>,
    ) {
        let total = quads.len();
        if total == 0 { return; }

        if total > self.supported_quantity {
            self.instances.destroy();
            self.supported_quantity = total;
            self.instances = context.device.create_buffer(/* ... */);
        }

        context.queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(quads));
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.constants, &[]);
        render_pass.set_vertex_buffer(0, self.instances.slice(..));
        render_pass.draw(0..6, 0..total as u32);
    }
}
```

### 6. Sugarloaf::render(): Batched overlay pass

```rust
// sugarloaf/src/sugarloaf.rs (inside render())
{
    let mut overlay_quads: Vec<Quad> = Vec::new();

    if let Some(glow) = self.state.cursor_glow_overlay {
        overlay_quads.push(glow);
    }
    if let Some(vi_overlay) = self.state.vi_mode_overlay {
        overlay_quads.push(vi_overlay);
    }
    if let Some(bell_overlay) = self.state.visual_bell_overlay {
        overlay_quads.push(bell_overlay);
    }
    if let Some(progress_bar) = self.state.progress_bar {
        overlay_quads.push(progress_bar);
    }

    if !overlay_quads.is_empty() {
        let mut overlay_pass = encoder.begin_render_pass(/* LoadOp::Load */);
        self.quad_brush.render_batch(&mut self.ctx, &overlay_quads, &mut overlay_pass);
    }
}
```

## Bug: Shared Instance Buffer Clobbering

### Root Cause

`QuadBrush` has a single `instances: wgpu::Buffer`. When `render_single()` was called for each overlay:

1. `render_single(cursor_glow)` → `queue.write_buffer(instances, glow_data)`
2. `render_single(vi_mode)` → `queue.write_buffer(instances, vi_data)`
3. `render_single(visual_bell)` → `queue.write_buffer(instances, bell_data)`
4. `render_single(progress_bar)` → `queue.write_buffer(instances, bar_data)`

`queue.write_buffer()` is queued and only executes when `queue.submit()` is called (after `encoder.finish()`). By that time, only the **last** write (progress bar) is in the buffer. All four render passes read from the same buffer, so all four drew the progress bar quad.

### Fix (Phase 1): Batched Overlay Rendering

Collect all overlay quads into one `Vec`, write once, draw all instances
in a single pass. The `render_single()` method is preserved (with a
WARNING doc comment) for cases where only one overlay exists, but the
overlay path in `render()` now exclusively uses `render_batch()`.

### Fix (Phase 2): Separate Overlay Instance Buffer

The Phase 1 fix solved the multi-write clobbering but introduced a
second latent bug: `render_batch()` and the main `render()` still shared
the same `instances: wgpu::Buffer`. When `render_batch()` needed to grow
the buffer (e.g., cursor trail adding more overlay quads than the initial
capacity), it called `self.instances.destroy()` and allocated a new one.
This destroyed the buffer that the main render pass had already bound via
`set_vertex_buffer()`. Since both passes are submitted in the same
`queue.submit()`, wgpu validation caught the destroyed buffer reference.

**Symptom**: Panic with `"Buffer with 'sugarloaf::quad batch instances'
label has been destroyed"` when opening a split (e.g., `Cmd+,` config
editor) while cursor trail quads were active.

**Fix**: Added a dedicated `overlay_instances` buffer and
`overlay_supported_quantity` counter to `QuadBrush`. The main `render()`
uses `self.instances` and `render_batch()` uses
`self.overlay_instances`, so growing one never invalidates the other.

## Files Changed

| File | Change |
|------|--------|
| `sugarloaf/src/sugarloaf/state.rs` | Added `cursor_glow_overlay: Option<Quad>` field, init, setter |
| `sugarloaf/src/sugarloaf.rs` | Added `set_cursor_glow_overlay()` API; replaced 4 separate overlay render passes with single batched pass |
| `sugarloaf/src/components/quad/mod.rs` | Added `render_batch()` method with separate `overlay_instances` buffer; added WARNING doc comment on `render_single()` |
| `frontends/rioterm/src/renderer/mod.rs` | Added cursor glow computation, trail state (`TrailEntry`, `VecDeque`), trail quad generation, `trail_animating` flag |
| `frontends/rioterm/src/context/grid.rs` | Added `current_position()` method to `ContextGrid` |
| `frontends/rioterm/src/application.rs` | Added `trail_animating` check to schedule redraws during trail fade-out |

## Dependencies

- CR-007 (overlay system architecture — this CR implements the simplest layer type)
- CR-004 (progress bar — the batched rendering fix ensures cursor glow and progress bar coexist)

## Testing

- **Visual**: Cursor glow should appear as a soft blue circle behind the cursor cell
- **Blinking**: Glow hides when cursor blinks off (respects `cursor.state.is_visible()`)
- **Splits**: Glow follows the focused pane's cursor position (uses `current_position()`)
- **Quick terminal**: Glow works in quick terminal overlay (position accessor checks QT state)
- **Coexistence**: Cursor glow + vi_mode_overlay + progress_bar all render simultaneously
- **Performance**: Single instanced draw call for all overlays (no per-overlay render pass overhead)

## Configuration (Implemented)

The cursor glow is fully configurable via `[cursor.glow]`:

```toml
[cursor.glow]
enabled = true          # toggle glow on/off
color = "cursor"        # "cursor" = derive from cursor/theme color, or hex like "#FF79C6"
intensity = 0.3         # base alpha for innermost layer (0.0–1.0)
radius = 1.5            # padding multiplier relative to cell width
layers = 3              # concentric glow layers (1–5) for bloom effect
trail = true            # enable cursor motion trail
trail-duration = 0.35   # trail fade-out duration in seconds (0.05–2.0)
trail-segments = 6      # max ghost segments in the trail (2–12)
```

### Multi-Layer Bloom

When `layers > 1`, concentric quads are rendered from outermost (largest,
most transparent) to innermost (smallest, brightest). The alpha for each
layer `i` (0 = outermost) is: `intensity * ((1 - t) * 0.8 + 0.2)` where
`t = (i + 1) / layers`. This creates a smooth radial falloff.

### Shape Awareness

The glow adapts to the cursor shape:
- **Block**: glow matches cell dimensions, fully rounded corners
- **Beam**: narrow vertical glow (2px wide), centered on cursor column
- **Underline**: flat horizontal glow (2px tall), centered on cursor row bottom

### Theme Integration

When `color = "cursor"` (default), the glow RGB is derived from
`named_colors.cursor` each frame, tracking theme changes and ANSI cursor
color overrides. Explicit hex values (e.g., `color = "#00BFFF"`) are
resolved once at config load.

### Cursor Motion Trail (Implemented)

When `trail = true`, the renderer records the cursor's pixel position each
frame into a `VecDeque<TrailEntry>`. When the cursor moves to a new position,
the old position is pushed onto the deque. Each frame:

1. **Eviction**: Entries older than `trail-duration` are removed from the front.
2. **Quad generation**: For each remaining entry, a shape-aware ghost quad is
   emitted with alpha that fades linearly from `intensity * 0.4` down to 0.
3. **Ordering**: Trail quads are prepended to the glow layers vec (rendered
   behind the current cursor glow).
4. **Animation loop**: While trail entries exist, `trail_animating` is set to
   `true` on the Renderer, and the application schedules continuous redraws
   until the trail fully fades out.

The trail adapts to cursor shape (block/beam/underline) and uses the same
glow color as the main cursor glow.

## Future Work

- **Interpolated trail**: Add intermediate ghost positions between large cursor
  jumps (e.g., page-up) for smoother visual continuity
- **Trail-only mode**: Allow trail without the static glow bloom layers

## References

- CR-007: Multi-Layer Transparent Click-Through Overlay
- CR-004: Graphical Progress Bar (OSC 9;4)
- wgpu `queue.write_buffer()` ordering: <https://docs.rs/wgpu/latest/wgpu/struct.Queue.html#method.write_buffer>
