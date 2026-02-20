# CR-007: Multi-Layer Transparent Click-Through Overlay Above Active Pane

**Status:** Proposed
**Date:** 2026-02-20
**Author:** wk

## Summary

Add a multi-layer, transparent, click-through overlay system that renders above the active pane. Each layer can contain Quad backgrounds **and** RichText content, enabling text labels, badges, tooltips, and styled content within overlay layers. Multiple layers stack in a controlled render order, all with completely transparent backgrounds by default, purely visual, and without intercepting any mouse or keyboard input.

## Motivation

1. **Text in overlays is essential**: Quad-only overlays can show colored rectangles and borders, but real UI overlays need text — labels on drop zones ("Drop here to split"), mode indicators ("RECORDING"), tooltip text, keyboard shortcut hints, or status badges. Without text support, overlay layers are limited to non-informational tinted rectangles.

2. **Multiple simultaneous visual effects**: A focus border tint + a selection rectangle + a tooltip with text may all need to render simultaneously over the same pane. Each layer must support both a Quad background and optional RichText content.

3. **Visual decoration without input blocking**: Overlays render on top of terminal content without preventing the user from clicking, selecting text, or scrolling in the pane underneath.

4. **Active pane awareness**: In split-pane layouts, a multi-layer overlay system provides compositing surfaces for focus indicators, mode badges, and status text tied to the active pane.

5. **Compositing foundation**: Supports future features such as:
   - Labeled drop zones ("Split Left" / "Split Right") during drag-and-drop
   - Mode indicator badge (e.g., "VI MODE", "SEARCH", "RECORDING")
   - Keyboard shortcut hint overlays with styled text
   - Tooltip popups with background + text content
   - Notification toasts with title + body text

## Architecture

```
 Renderer::run()                                 Sugarloaf::render()
 ┌───────────────────────────────┐               ┌────────────────────────────────────┐
 │                               │               │                                    │
 │ 1. Identify active pane       │               │ Main render pass:                  │
 │ 2. Build OverlayLayer stack:  │               │   - Bottom layer (bg image)        │
 │    ┌────────────────────────┐ │               │   - Quads (borders, backgrounds)   │
 │    │ Layer 0: focus border  │ │               │   - Rich text (terminal content)   │
 │    │   quad only            │ │               │   - Overlay RichText objects ← NEW │
 │    │                        │ │               │                                    │
 │    │ Layer 1: mode badge    │ │               │ Single-quad overlay passes:        │
 │    │   quad + rich_text     │ │               │   - vi_mode_overlay                │
 │    │                        │ │               │   - visual_bell_overlay             │
 │    │ Layer 2: tooltip       │ │               │   - progress_bar                   │
 │    │   quad + rich_text     │ │               │                                    │
 │    └────────────────────────┘ │               │ Multi-layer quad overlay pass: NEW │
 │ 3. Set text content via       │               │   - All layer quads in one         │
 │    Content builder API        │               │     instanced GPU draw call        │
 │ 4. set_overlay_layers(vec)    │──set_*()─────▶│                                    │
 │                               │               │ Post-processing filters            │
 └───────────────────────────────┘               └────────────────────────────────────┘

 Layer structure (each layer = optional Quad + optional RichText):
 ┌──────────────────────────────────────────────────────────────────┐
 │                                                                  │
 │  ┌─ Layer 2 (topmost) ───────────────────────────────────────┐  │
 │  │  Quad: tooltip bg [0.1, 0.1, 0.1, 0.9]                   │  │
 │  │  RichText: "Press Ctrl+D to close"                        │  │
 │  │  ┌─ Layer 1 ───────────────────────────────────────────┐  │  │
 │  │  │  Quad: badge bg [0.2, 0.4, 0.8, 0.85]              │  │  │
 │  │  │  RichText: "VI MODE"                                │  │  │
 │  │  │  ┌─ Layer 0 ─────────────────────────────────────┐  │  │  │
 │  │  │  │  Quad: focus border, no fill                  │  │  │  │
 │  │  │  │  RichText: (none)                             │  │  │  │
 │  │  │  │                                               │  │  │  │
 │  │  │  │    Terminal content visible through all layers │  │  │  │
 │  │  │  └───────────────────────────────────────────────┘  │  │  │
 │  │  └─────────────────────────────────────────────────────┘  │  │
 │  └───────────────────────────────────────────────────────────┘  │
 │                                                                  │
 └──────────────────────────────────────────────────────────────────┘

 Mouse/keyboard input pipeline (UNCHANGED):
 ┌────────────────────────────────────────────────────────────────────────┐
 │ application.rs::WindowEvent::MouseInput                               │
 │   → screen.select_current_based_on_mouse()                           │
 │     → ContextGrid checks RichText positions + Context dimensions     │
 │   → screen.on_left_click() / mouse_report()                          │
 │                                                                       │
 │ Quads and RichText objects are GPU-only. The input system only checks │
 │ pane RichText positions + Context dimensions. Overlay RichText IDs    │
 │ are not in ContextGrid, so they are never hit-tested.                 │
 │ ALL overlay layers are inherently click-through.                      │
 └────────────────────────────────────────────────────────────────────────┘
```

### Why all layers are click-through

Rio's input and rendering systems are fully decoupled:

- **Input path**: `application.rs` dispatches mouse/keyboard events to `Screen`, which delegates to `ContextManager` / `ContextGrid`. Hit-testing uses pane `RichText` object positions and `Context` dimensions only. `ContextGrid::select_current_based_on_mouse()` iterates `ContextGridItem`s and matches against their `rich_text_object` positions — it has no knowledge of overlay RichText IDs.
- **Rendering path**: `Quad` and `RichText` objects are GPU-only drawing primitives. Quads are written to a wgpu vertex buffer. RichText objects are shaped and rasterized via `RichTextBrush`. Neither has any input handling.

Since the input pipeline never inspects overlay Quad positions or overlay RichText IDs, all overlay layers (both Quads and text) are inherently click-through.

## Design

### The `OverlayLayer` Struct

Each overlay layer is represented as a composite of an optional Quad background and optional RichText content:

```rust
/// A single overlay layer that can contain a background quad and/or text content.
/// Both fields are optional: a layer can be quad-only, text-only, or both.
pub struct OverlayLayer {
    /// Background/decoration quad (position, size, color, border, shadow).
    /// Set to `None` for text-only layers.
    pub quad: Option<Quad>,

    /// RichText identifier linking to shaped text in Content.
    /// Set to `None` for quad-only layers (focus tints, selection rectangles).
    pub rich_text: Option<RichText>,
}
```

This design follows the leader menu pattern, where `Object::Quad` and `Object::RichText` are combined to form a complete visual element (background + text). The difference is that overlay layers are rendered in a dedicated pass order rather than in the main Object pipeline.

### Why not just extend `Object`?

The existing `Object` enum (`Object::Quad` / `Object::RichText`) is rendered in the main render pass alongside terminal content. Overlay layers need to render **on top of** all terminal content and existing overlays. Using a separate `OverlayLayer` struct with its own render pass ensures correct z-ordering without modifying the main Object pipeline.

### Text Rendering Strategy: Two-Phase Approach

RichText rendering in Rio is a batch operation — `RichTextBrush::prepare()` processes ALL `RichText` objects into a single vertex buffer, and `render()` draws them all in one draw call. There is no `render_single()` for RichText.

This means overlay text **must participate in the main RichText batch**. The design uses a two-phase approach:

```
Phase 1 — Main render pass:
  RichTextBrush::prepare() processes ALL RichTexts (pane content + overlay text)
  RichTextBrush::render() draws ALL text in one draw call
  → Overlay text is drawn HERE, composited with pane text

Phase 2 — Overlay quad render pass:
  QuadBrush::render_slice() draws overlay quad backgrounds
  → Overlay backgrounds are drawn HERE, on top of all text
```

**Important consequence**: Overlay text renders in the main pass (Phase 1) while overlay backgrounds render in the overlay pass (Phase 2). This means overlay quad backgrounds appear **on top of** overlay text. To achieve the expected "background behind text" visual, overlay quads that serve as text backgrounds must be injected into the main quad pass instead (via the `objects` Vec), not in the overlay quad render pass.

The actual render order for a layer with both quad and text is:

```
Main pass:
  1. Pane background quads
  2. Pane terminal text
  3. Overlay background quads (injected as Object::Quad into objects Vec)
  4. Overlay text (injected as Object::RichText into objects Vec)

Overlay quad pass:
  5. Overlay-only quads (borders, highlights, tints — no backing text)
```

### Layer Types and Render Path Routing

Based on the two-phase architecture, each layer's components are routed differently:

| Layer Configuration | Quad Route | Text Route |
|---|---|---|
| Quad-only (focus tint, selection rect) | Overlay quad pass (`render_slice`) | — |
| Text-only (floating label) | — | Main pass (via `objects` Vec) |
| Quad + Text (badge, tooltip) | Main pass (via `objects` Vec as background) | Main pass (via `objects` Vec) |

This means `OverlayLayer` components are split at render time:

- Layers with **quad + text**: Both components go into the `objects` Vec (rendered in main pass, text on top of quad via Object ordering)
- Layers with **quad only**: Quad goes into the overlay quad render pass (rendered on top of everything, ideal for tints and borders)
- Layers with **text only**: RichText goes into the `objects` Vec

### Layer Ordering

Layers are rendered in Vec order (index 0 first, index N last). Within the main pass, Objects from lower-indexed layers are pushed before higher-indexed layers, ensuring correct z-ordering. Overlay-only quads follow the same index order in the overlay pass.

## Implementation Details

### 1. Define the `OverlayLayer` Struct

**File:** `sugarloaf/src/sugarloaf/primitives.rs`

```rust
/// A single overlay layer that can contain a background quad and/or text content.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayLayer {
    /// Background/decoration quad. None for text-only layers.
    pub quad: Option<Quad>,
    /// RichText reference (id + position). None for quad-only layers.
    pub rich_text: Option<RichText>,
}
```

### 2. Add `render_slice()` Method to `QuadBrush`

**File:** `sugarloaf/src/components/quad/mod.rs`

```rust
/// Render multiple quads from a slice in a single instanced draw call.
/// Used for multi-layer overlay passes where quads are not stored in SugarState.
pub fn render_slice<'a>(
    &'a mut self,
    context: &mut Context,
    quads: &[Quad],
    render_pass: &mut wgpu::RenderPass<'a>,
) {
    let total = quads.len();
    if total == 0 {
        return;
    }

    if total > self.supported_quantity {
        self.instances.destroy();
        self.supported_quantity = total;
        self.instances = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sugarloaf::quad instances"),
            size: mem::size_of::<Quad>() as u64 * self.supported_quantity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    let instance_bytes = bytemuck::cast_slice(quads);
    context.queue.write_buffer(&self.instances, 0, instance_bytes);

    render_pass.set_pipeline(&self.pipeline);
    render_pass.set_bind_group(0, &self.constants, &[]);
    render_pass.set_vertex_buffer(0, self.instances.slice(..));
    render_pass.draw(0..6, 0..total as u32);
}
```

### 3. Add Overlay Layer Fields to `SugarState`

**File:** `sugarloaf/src/sugarloaf/state.rs`

```rust
pub struct SugarState {
    // ... existing fields ...
    pub visual_bell_overlay: Option<Quad>,
    pub vi_mode_overlay: Option<Quad>,
    pub progress_bar: Option<Quad>,
    pub overlay_layers: Vec<OverlayLayer>,     // NEW: multi-layer overlay stack
    pub overlay_only_quads: Vec<Quad>,         // NEW: quad-only layers for overlay pass
}
```

Initialize in constructor:

```rust
SugarState {
    // ... existing fields ...
    overlay_layers: Vec::new(),
    overlay_only_quads: Vec::new(),
}
```

Setter:

```rust
pub fn set_overlay_layers(&mut self, layers: Vec<OverlayLayer>) {
    self.overlay_layers = layers;
}

pub fn set_overlay_only_quads(&mut self, quads: Vec<Quad>) {
    self.overlay_only_quads = quads;
}
```

### 4. Add Public API on `Sugarloaf`

**File:** `sugarloaf/src/sugarloaf.rs`

```rust
pub fn set_overlay_layers(&mut self, layers: Vec<OverlayLayer>) {
    self.state.set_overlay_layers(layers);
}

pub fn set_overlay_only_quads(&mut self, quads: Vec<Quad>) {
    self.state.set_overlay_only_quads(quads);
}
```

### 5. Add Overlay Quad Render Pass

**File:** `sugarloaf/src/sugarloaf.rs` (after the progress bar render pass, ~line 528)

```rust
// Multi-layer click-through overlay quads (borders, tints, highlights)
if !self.state.overlay_only_quads.is_empty() {
    let mut overlay_pass =
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay_layers"),
            color_attachments: &[Some(
                wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                },
            )],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    self.quad_brush.render_slice(
        &mut self.ctx,
        &self.state.overlay_only_quads,
        &mut overlay_pass,
    );
}
```

### 6. Construct Overlay Layers in `Renderer::run()`

**File:** `frontends/rioterm/src/renderer/mod.rs`

The renderer builds the layer stack, then routes each layer's components to the correct render path:

```rust
// Multi-layer click-through overlay above active pane
let (overlay_objects, overlay_only_quads) = {
    let grid = context_manager.current_grid();
    let active_key = grid.current;
    let mut obj_vec: Vec<Object> = Vec::new();
    let mut quad_vec: Vec<Quad> = Vec::new();

    if let Some(item) = grid.inner().get(&active_key) {
        let rich_text_obj = &item.rich_text_object;
        let pane_position = rich_text_obj.position;
        let pane_size = [item.val.dimension.width, item.val.dimension.height];

        // Build layer stack
        let layers = self.build_overlay_layers(sugarloaf, pane_position, pane_size);

        // Route each layer's components to the correct render path
        for layer in &layers {
            match (&layer.quad, &layer.rich_text) {
                // Quad + Text: both go to main pass via objects Vec
                (Some(quad), Some(rich_text)) => {
                    obj_vec.push(Object::Quad(*quad));
                    obj_vec.push(Object::RichText(*rich_text));
                }
                // Text only: goes to main pass via objects Vec
                (None, Some(rich_text)) => {
                    obj_vec.push(Object::RichText(*rich_text));
                }
                // Quad only: goes to overlay quad render pass
                (Some(quad), None) => {
                    quad_vec.push(*quad);
                }
                // Empty layer: skip
                (None, None) => {}
            }
        }
    }

    (obj_vec, quad_vec)
};

// Inject overlay objects into the main objects Vec (for text + text-background layers)
objects.extend(overlay_objects);

// Set overlay-only quads for the dedicated overlay render pass
sugarloaf.set_overlay_only_quads(overlay_only_quads);
```

### 7. Text Content Management

**File:** `frontends/rioterm/src/renderer/mod.rs`

Overlay RichText content is managed through the existing `Content` builder API, following the leader menu pattern. The `Renderer` struct holds persistent RichText IDs for overlay text:

```rust
pub struct Renderer {
    // ... existing fields ...
    overlay_rich_text_ids: Vec<usize>,  // Persistent RichText IDs for overlay text
}
```

Creating and updating overlay text:

```rust
impl Renderer {
    /// Build the overlay layer stack for the active pane.
    fn build_overlay_layers(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        pane_position: [f32; 2],
        pane_size: [f32; 2],
    ) -> Vec<OverlayLayer> {
        let mut layers = Vec::new();

        // Layer 0: Focus border (quad-only, no text)
        layers.push(OverlayLayer {
            quad: Some(Quad {
                position: pane_position,
                size: pane_size,
                color: [0.0, 0.0, 0.0, 0.0],
                ..Quad::default()
            }),
            rich_text: None,
        });

        // Layer 1 example: Mode badge with text
        // Allocate a RichText ID if not already created
        if self.overlay_rich_text_ids.is_empty() {
            let rt_id = sugarloaf.create_rich_text();
            sugarloaf.set_rich_text_font_size(&rt_id, 12.0);
            self.overlay_rich_text_ids.push(rt_id);
        }

        // Update text content via Content builder API
        let rt_id = self.overlay_rich_text_ids[0];
        let content = sugarloaf.content();
        let line = content.sel(rt_id);
        line.clear();
        line.new_line();
        line.add_text("VI MODE", FragmentStyle {
            color: [1.0, 1.0, 1.0, 1.0],
            ..FragmentStyle::default()
        });
        line.build();

        // Badge position: top-right corner of active pane
        let badge_w = 80.0;
        let badge_h = 24.0;
        let badge_x = pane_position[0] + pane_size[0] - badge_w - 8.0;
        let badge_y = pane_position[1] + 8.0;

        layers.push(OverlayLayer {
            quad: Some(Quad {
                position: [badge_x, badge_y],
                size: [badge_w, badge_h],
                color: [0.2, 0.4, 0.8, 0.85],
                border_radius: [4.0; 4],
                ..Quad::default()
            }),
            rich_text: Some(RichText {
                id: rt_id,
                position: [badge_x + 8.0, badge_y + 4.0],
                lines: None,
            }),
        });

        layers
    }
}
```

### 8. No Input Changes Required

All layers (including text layers) are click-through by architecture:
- `ContextGrid::select_current_based_on_mouse()` only matches against pane `ContextGridItem` entries
- Overlay RichText IDs are not registered in `ContextGrid::inner`, so they are never hit-tested
- No modifications to `application.rs`, `screen/mod.rs`, or any input handling code are needed

## Render Pass Order

After this change, the GPU render pass order becomes:

```
1. Main pass          (LoadOp::Clear)  — bg image, quads, rich text,
                                         overlay bg quads + overlay text  ← NEW objects
2. vi_mode_overlay    (LoadOp::Load)   — full-window vi mode tint
3. visual_bell        (LoadOp::Load)   — full-window bell flash
4. progress_bar       (LoadOp::Load)   — top bar status indicator
5. overlay_only_quads (LoadOp::Load)   — quad-only overlay layers  ← NEW pass
6. Filters            (LoadOp::Load)   — post-processing shaders
```

### Render Path Per Layer Type

| Layer Type | Quad Background | Text | Render Location |
|---|---|---|---|
| Quad-only (tint, border, selection) | Overlay pass (step 5) | — | On top of all content |
| Text-only (floating label) | — | Main pass (step 1) | Mixed with terminal text |
| Quad + Text (badge, tooltip) | Main pass (step 1) | Main pass (step 1) | Quad behind text, both above pane content |

### Why This Split Works

The split is necessary because `RichTextBrush` batches all text into a single `prepare()` + `render()` cycle within the main pass. There is no `render_single()` for RichText, so overlay text cannot be rendered in a separate pass.

For layers with **Quad + Text**: Both go to the `objects` Vec. Object ordering (`Object::Quad` first, then `Object::RichText`) ensures the quad background renders behind the text, following the same pattern as the leader menu.

For layers with **Quad only**: The quad goes to the dedicated overlay pass, rendering on top of everything — ideal for tints, borders, and highlights that should cover terminal text.

## `OverlayLayer` Configuration

### Quad Fields Per Layer

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `color` | `[f32; 4]` | `[0.0; 4]` | RGBA fill. `color[3]` = transparency |
| `position` | `[f32; 2]` | `[0.0; 2]` | Top-left corner |
| `size` | `[f32; 2]` | `[0.0; 2]` | Width and height |
| `border_color` | `[f32; 4]` | `[0.0; 4]` | Border color |
| `border_radius` | `[f32; 4]` | `[0.0; 4]` | Corner rounding |
| `border_width` | `f32` | `0.0` | Border stroke width |
| `shadow_color` | `[f32; 4]` | `[0.0; 4]` | Drop shadow color |
| `shadow_offset` | `[f32; 2]` | `[0.0; 2]` | Shadow offset |
| `shadow_blur_radius` | `f32` | `0.0` | Shadow blur |

### RichText Fields Per Layer

| Field | Type | Purpose |
|-------|------|---------|
| `id` | `usize` | References a `BuilderState` in `Content.states` — holds shaped glyph data |
| `position` | `[f32; 2]` | Text position (typically offset from quad position for padding) |
| `lines` | `Option<RichTextLinesRange>` | Optional line range filter (`None` = all lines) |

### Text Content via `Content` Builder API

Text content is set through the existing builder pattern (same as leader menu):

```rust
let content = sugarloaf.content();
let line = content.sel(rich_text_id);  // Select BuilderState by ID
line.clear();                           // Clear previous content
line.new_line();                        // Start new line
line.add_text("Label", style);         // Add styled text fragment
line.build();                           // Trigger text shaping
```

`FragmentStyle` controls per-fragment appearance:
- `color: [f32; 4]` — text color with alpha
- `font_size: f32` — font size override
- `weight`, `style` — bold, italic, etc.
- `underline`, `strikethrough` — decorations

## Multi-Layer Examples

### Example 1: Focus border (quad-only, 1 layer)

```rust
vec![OverlayLayer {
    quad: Some(Quad {
        position: pane_position,
        size: pane_size,
        color: [0.0, 0.0, 0.0, 0.0],
        border_color: [0.3, 0.5, 1.0, 0.6],
        border_width: 2.0,
        ..Quad::default()
    }),
    rich_text: None,
}]
```

### Example 2: Mode badge with text (quad + text, 1 layer)

```rust
vec![OverlayLayer {
    quad: Some(Quad {
        position: [badge_x, badge_y],
        size: [80.0, 24.0],
        color: [0.2, 0.4, 0.8, 0.85],
        border_radius: [4.0; 4],
        ..Quad::default()
    }),
    rich_text: Some(RichText {
        id: mode_badge_rt_id,
        position: [badge_x + 8.0, badge_y + 4.0],
        lines: None,
    }),
}]
// Content: "VI MODE" in white, 12px
```

### Example 3: Focus border + labeled drop zones (3 layers, mixed)

```rust
vec![
    // Layer 0: Focus border (quad-only → overlay pass)
    OverlayLayer {
        quad: Some(Quad {
            position: pane_position,
            size: pane_size,
            color: [0.0, 0.0, 0.0, 0.0],
            border_color: [0.3, 0.5, 1.0, 0.4],
            border_width: 2.0,
            ..Quad::default()
        }),
        rich_text: None,
    },
    // Layer 1: "Split Left" drop zone (quad + text → main pass)
    OverlayLayer {
        quad: Some(Quad {
            position: pane_position,
            size: [pane_size[0] / 2.0, pane_size[1]],
            color: [0.0, 0.8, 0.4, 0.1],
            border_color: [0.0, 0.8, 0.4, 0.4],
            border_width: 1.0,
            ..Quad::default()
        }),
        rich_text: Some(RichText {
            id: left_label_rt_id,           // Content: "Split Left"
            position: [center_left_x, center_y],
            lines: None,
        }),
    },
    // Layer 2: "Split Right" drop zone (quad + text → main pass)
    OverlayLayer {
        quad: Some(Quad {
            position: [pane_position[0] + pane_size[0] / 2.0, pane_position[1]],
            size: [pane_size[0] / 2.0, pane_size[1]],
            color: [1.0, 0.6, 0.0, 0.1],
            border_color: [1.0, 0.6, 0.0, 0.4],
            border_width: 1.0,
            ..Quad::default()
        }),
        rich_text: Some(RichText {
            id: right_label_rt_id,          // Content: "Split Right"
            position: [center_right_x, center_y],
            lines: None,
        }),
    },
]
```

### Example 4: Notification toast (quad + multi-line text)

```rust
// Set text content
let content = sugarloaf.content();
let line = content.sel(toast_rt_id);
line.clear();
line.new_line();
line.add_text("Build Complete", title_style);  // Bold white
line.new_line();
line.add_text("cargo build finished in 2.3s", body_style);  // Gray
line.build();

vec![OverlayLayer {
    quad: Some(Quad {
        position: [pane_x + pane_w - 260.0, pane_y + pane_h - 70.0],
        size: [250.0, 60.0],
        color: [0.12, 0.12, 0.14, 0.95],
        border_radius: [6.0; 4],
        shadow_color: [0.0, 0.0, 0.0, 0.3],
        shadow_offset: [0.0, 2.0],
        shadow_blur_radius: 8.0,
        ..Quad::default()
    }),
    rich_text: Some(RichText {
        id: toast_rt_id,
        position: [pane_x + pane_w - 248.0, pane_y + pane_h - 62.0],
        lines: None,
    }),
}]
```

## RichText ID Lifecycle

Overlay RichText IDs are managed by the `Renderer` and follow the same lifecycle pattern as the leader menu:

| Phase | Action | API |
|---|---|---|
| **Create** | Allocate a `BuilderState` ID (once, on first use) | `sugarloaf.create_rich_text()` |
| **Configure** | Set font size for the RichText | `sugarloaf.set_rich_text_font_size(&id, size)` |
| **Update** | Set/change text content each frame | `content.sel(id).clear().new_line().add_text(...).build()` |
| **Render** | Include `RichText { id, position, lines }` in overlay layer | Automatic via `objects` Vec |
| **Destroy** | Remove when overlay is dismissed | `content.remove_state(&id)` |

IDs are persistent across frames. Text content can be updated every frame (e.g., for animated labels) or only when content changes (e.g., mode badge text changes on mode switch). The `BuilderState::last_update` field tracks whether reshaping is needed, so unchanged text has zero CPU cost on subsequent frames.

## Performance

| Metric | Impact |
|--------|--------|
| Render passes | +1 (only when quad-only layers exist) |
| Draw calls per frame | +1 instanced draw call for overlay quads; text batched into existing RichText draw call |
| GPU buffer writes | +1 `write_buffer` for overlay quads; text vertices already in RichText buffer |
| CPU overhead | `Vec<OverlayLayer>` construction + routing; text shaping only on content change |
| Memory | `sizeof(Quad) * N` on GPU for overlay quads; RichText states in Content hashmap |
| Zero-layer cost | None — empty Vecs skip all overlay work |
| Text reshaping | Only when `BuilderState::last_update` is not `Noop` (cached across frames) |

## Files Changed

| File | Changes |
|------|---------|
| `sugarloaf/src/sugarloaf/primitives.rs` | Add `OverlayLayer` struct definition |
| `sugarloaf/src/components/quad/mod.rs` | Add `render_slice(&[Quad])` method for multi-quad overlay rendering |
| `sugarloaf/src/sugarloaf/state.rs` | Add `overlay_layers: Vec<OverlayLayer>`, `overlay_only_quads: Vec<Quad>` fields, constructors, setters |
| `sugarloaf/src/sugarloaf.rs` | Add `set_overlay_layers()`, `set_overlay_only_quads()` methods; add overlay quad render pass |
| `frontends/rioterm/src/renderer/mod.rs` | Add `overlay_rich_text_ids: Vec<usize>` to `Renderer`; add `build_overlay_layers()` method; route layer components to correct render paths; inject overlay objects into `objects` Vec |

## Dependencies

None. Uses only existing infrastructure:
- `Quad` struct from `sugarloaf/src/components/quad/mod.rs`
- `RichText` struct from `sugarloaf/src/sugarloaf/primitives.rs`
- `Content` builder API from `sugarloaf/src/layout/content.rs`
- `QuadBrush` GPU buffer and instanced drawing (extended with `render_slice()`)
- `RichTextBrush` batch rendering (existing `prepare()` + `render()` cycle)
- `ContextGrid::current` and `ContextGridItem` for active pane geometry
- Standard wgpu render pass with `LoadOp::Load`

## Testing

### Visual Verification

1. **Quad-only layer**: Push a single layer with visible border, no text. Verify border renders on top of terminal content and is click-through.

2. **Text-only layer**: Push a single layer with RichText, no quad. Verify text renders at specified position and is click-through.

3. **Quad + Text layer**: Push a layer with both. Verify quad background renders behind text, both are positioned correctly.

4. **Multi-layer stack**: Push 3 layers (quad-only + quad+text + text-only). Verify all render in correct z-order.

5. **Split panes**: Verify overlay tracks active pane, switches on focus change, click-through works on inactive panes.

6. **Dynamic text update**: Change text content every frame (e.g., timer). Verify no flicker or memory growth.

7. **Zero layers**: Empty Vec produces no render pass and no objects — verify with wgpu debug labels.

### Click-Through Verification

With visible overlay layers (including text layers):
- Left-click through text should set cursor position / start selection
- Double-click through text should select a word
- Scroll through overlay should work normally
- Mouse reporting to terminal apps should work normally

### Text Rendering Verification

1. **Single-line text**: "VI MODE" in a badge — verify font, color, position
2. **Multi-line text**: Title + body in a toast — verify line spacing, wrapping
3. **Styled text**: Mixed bold/color fragments — verify `FragmentStyle` applied correctly
4. **Font size**: 12px overlay text alongside 14px terminal text — verify independent sizing
5. **Text update**: Change label from "RECORDING" to "STOPPED" — verify clean transition

### Performance Verification

1. Push 0 layers → verify no render pass, no extra objects
2. Push 1 quad-only layer → verify 1 overlay render pass, 0 extra objects
3. Push 1 quad+text layer → verify 0 overlay render pass (routed to main), 2 extra objects
4. Push 5 mixed layers → verify correct routing split
5. Rapid text updates → verify text reshaping is cached when content unchanged

## Relationship to Existing Overlays

| Overlay | Storage | Quad Render | Text Render | Pass |
|---------|---------|-------------|-------------|------|
| Vi mode | `Option<Quad>` | `render_single()` | — | Own pass |
| Visual bell | `Option<Quad>` | `render_single()` | — | Own pass |
| Progress bar | `Option<Quad>` | `render_single()` | — | Own pass |
| Leader menu | `Vec<Object>` | Main pass | Main pass | Main pass |
| **Overlay layers** | **`Vec<OverlayLayer>`** | **Main pass or overlay pass** | **Main pass** | **Split routing** |

The overlay layer system follows the leader menu's combined Quad + RichText pattern for layers that need text, and uses a dedicated overlay pass for quad-only layers that need to render on top of everything.

## Future Enhancements

1. **Dedicated overlay text pass**: Add `RichTextBrush::prepare_overlay()` + `render_overlay()` methods to support rendering overlay text in a separate pass, eliminating the main-pass routing split
2. **Named layers**: Layer identifiers for targeted updates without rebuilding the full Vec
3. **Layer animation**: Per-layer opacity/position animation with easing curves
4. **Per-pane layer stacks**: Overlay layers on inactive panes (notification badges, status indicators)
5. **Interactive overlay layers**: Optional input capture for layers that need click handling (e.g., button overlays), with explicit opt-in via an `interactive: bool` field
6. **Configurable presets**: User-configurable overlay styles via config (focus tint color, badge position, font size)
