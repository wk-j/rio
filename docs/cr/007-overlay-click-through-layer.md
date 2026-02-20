# CR-007: Multi-Layer Transparent Click-Through Overlay Above Active Pane

**Status:** Proposed
**Date:** 2026-02-20
**Author:** wk

## Summary

Add a multi-layer, transparent, click-through overlay system that renders above the active pane. Each layer can contain Quad backgrounds, RichText content, **and/or images**, enabling text labels, badges, tooltips, icons, thumbnails, and styled content within overlay layers. Multiple layers stack in a controlled render order, all with completely transparent backgrounds by default, purely visual, and without intercepting any mouse or keyboard input.

Additionally, the overlay system supports **command output layers** — a special layer type that spawns a predefined command (e.g., `top`, `htop`, `git log`) in a real PTY and renders its live terminal output within an overlay. Command output layers are triggered by configurable keyboard shortcuts and reuse the Quick Terminal infrastructure (`Context` + `Crosswords` + `Machine` IO thread) to provide full ANSI rendering, scrollback, and optional interactivity.

## Motivation

1. **Rich overlay content**: Quad-only overlays can show colored rectangles and borders, but real UI overlays need text and images — labels on drop zones ("Drop here to split"), mode indicators ("RECORDING"), tooltip text, icons, thumbnails, or status badges. Without text and image support, overlay layers are limited to non-informational tinted rectangles.

2. **Multiple simultaneous visual effects**: A focus border tint + a selection rectangle + a tooltip with text + an icon may all need to render simultaneously over the same pane. Each layer must support a Quad background, optional RichText content, and optional image content.

3. **Visual decoration without input blocking**: Overlays render on top of terminal content without preventing the user from clicking, selecting text, or scrolling in the pane underneath.

4. **Active pane awareness**: In split-pane layouts, a multi-layer overlay system provides compositing surfaces for focus indicators, mode badges, icons, and status text tied to the active pane.

5. **Quick command output**: Users want to glance at system monitors (`top`, `htop`), git status (`git log --oneline`), or other command output without leaving the current terminal session. A keybinding-triggered overlay that spawns a command in a real PTY and renders its live output on top of the active pane — then dismisses on toggle or process exit — provides this workflow without disrupting the pane layout.

6. **Compositing foundation**: Supports future features such as:
   - Labeled drop zones ("Split Left" / "Split Right") during drag-and-drop
   - Mode indicator badge (e.g., "VI MODE", "SEARCH", "RECORDING")
   - Keyboard shortcut hint overlays with styled text
   - Tooltip popups with background + text + icon content
   - Notification toasts with title + body text + app icon
   - Image preview thumbnails (e.g., hover-preview of file icons, image assets)
   - Overlay watermarks or branding images
   - Predefined command output overlays (e.g., `top`, `htop`, `git log`)

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
 │    │                        │ │               │   - Overlay images (LayerBrush)    │
 │    │ Layer 1: mode badge    │ │               │                                    │
 │    │   quad + rich_text     │ │               │ Single-quad overlay passes:        │
 │    │                        │ │               │   - vi_mode_overlay                │
 │    │ Layer 2: icon layer    │ │               │   - visual_bell_overlay             │
 │    │   quad + image         │ │               │   - progress_bar                   │
 │    │                        │ │               │                                    │
 │    │ Layer 3: tooltip       │ │               │ Multi-layer quad overlay pass: NEW │
 │    │   quad + rich_text     │ │               │   - All layer quads in one         │
 │    └────────────────────────┘ │               │     instanced GPU draw call        │
 │ 3. Set text content via       │               │                                    │
 │    Content builder API        │               │ Image overlay pass: NEW            │
 │ 4. Prepare images via         │               │   - LayerBrush.render() into       │
 │    LayerBrush.prepare()       │               │     LoadOp::Load pass              │
 │ 5. set_overlay_layers(vec)    │──set_*()─────▶│                                    │
 │                               │               │ Post-processing filters            │
 └───────────────────────────────┘               └────────────────────────────────────┘

 Layer structure (each layer = optional Quad + optional RichText + optional Image):
 ┌──────────────────────────────────────────────────────────────────┐
 │                                                                  │
 │  ┌─ Layer 3 (topmost) ───────────────────────────────────────┐  │
 │  │  Quad: tooltip bg [0.1, 0.1, 0.1, 0.9]                   │  │
 │  │  RichText: "Press Ctrl+D to close"                        │  │
 │  │  Image: (none)                                            │  │
 │  │  ┌─ Layer 2 ───────────────────────────────────────────┐  │  │
 │  │  │  Quad: icon bg [0.15, 0.15, 0.17, 0.9]             │  │  │
 │  │  │  RichText: (none)                                   │  │  │
 │  │  │  Image: icon.png (32x32)                            │  │  │
 │  │  │  ┌─ Layer 1 ─────────────────────────────────────┐  │  │  │
 │  │  │  │  Quad: badge bg [0.2, 0.4, 0.8, 0.85]        │  │  │  │
 │  │  │  │  RichText: "VI MODE"                          │  │  │  │
 │  │  │  │  Image: (none)                                │  │  │  │
 │  │  │  │  ┌─ Layer 0 ───────────────────────────────┐  │  │  │  │
 │  │  │  │  │  Quad: focus border, no fill            │  │  │  │  │
 │  │  │  │  │  RichText: (none)                       │  │  │  │  │
 │  │  │  │  │  Image: (none)                          │  │  │  │  │
 │  │  │  │  │                                         │  │  │  │  │
 │  │  │  │  │  Terminal content visible through       │  │  │  │  │
 │  │  │  │  │  all layers (alpha compositing)         │  │  │  │  │
 │  │  │  │  └─────────────────────────────────────────┘  │  │  │  │
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
 │ Quads, RichText, and images are GPU-only. The input system only       │
 │ checks pane RichText positions + Context dimensions. Overlay          │
 │ RichText IDs and image layers are not in ContextGrid.                 │
 │ ALL overlay layers are inherently click-through.                      │
 └────────────────────────────────────────────────────────────────────────┘
```

### Why all layers are click-through

Rio's input and rendering systems are fully decoupled:

- **Input path**: `application.rs` dispatches mouse/keyboard events to `Screen`, which delegates to `ContextManager` / `ContextGrid`. Hit-testing uses pane `RichText` object positions and `Context` dimensions only. `ContextGrid::select_current_based_on_mouse()` iterates `ContextGridItem`s and matches against their `rich_text_object` positions — it has no knowledge of overlay RichText IDs.
- **Rendering path**: `Quad`, `RichText`, and image layers are GPU-only drawing primitives. Quads are written to a wgpu vertex buffer. RichText objects are shaped and rasterized via `RichTextBrush`. Images are uploaded to a texture atlas and rendered via `LayerBrush`. None have any input handling.

Since the input pipeline never inspects overlay Quad positions, overlay RichText IDs, or overlay image layers, all overlay layers (Quads, text, and images) are inherently click-through.

## Design

### The `OverlayLayer` Struct

Each overlay layer is represented as a composite of an optional Quad background, optional RichText content, and optional image:

```rust
/// A single overlay layer that can contain a background quad, text, and/or an image.
/// All fields are optional: a layer can be any combination of quad, text, and image.
pub struct OverlayLayer {
    /// Background/decoration quad (position, size, color, border, shadow).
    /// Set to `None` for text-only or image-only layers.
    pub quad: Option<Quad>,

    /// RichText identifier linking to shaped text in Content.
    /// Set to `None` for quad-only or image-only layers.
    pub rich_text: Option<RichText>,

    /// Image to render in this layer (handle + bounds).
    /// Set to `None` for quad-only or text-only layers.
    pub image: Option<OverlayImage>,
}

/// Image reference for an overlay layer.
pub struct OverlayImage {
    /// Image handle (from path, bytes, or RGBA pixels).
    pub handle: image::Handle,
    /// Screen-space bounds where the image is rendered.
    pub bounds: Rectangle,
}
```

This design follows the leader menu pattern for Quad + RichText, and extends it with `OverlayImage` that leverages the existing `LayerBrush` infrastructure. The `image::Handle` supports three sources: file path, in-memory bytes, and raw RGBA pixels — the same sources used by background images and inline images (sixel/iTerm2).

### Why not just extend `Object`?

The existing `Object` enum (`Object::Quad` / `Object::RichText`) is rendered in the main render pass alongside terminal content. Overlay layers need to render **on top of** all terminal content and existing overlays. Additionally, `Object` has no image variant — images flow through a separate `LayerBrush` pipeline. Using a separate `OverlayLayer` struct that combines all three content types (quad, text, image) with its own render pass routing ensures correct z-ordering without modifying the main Object pipeline or the existing image rendering flow.

### Rendering Strategy: Multi-Phase Approach

Each content type in Rio has its own rendering brush and constraints:

| Content Type | Brush | Constraint |
|---|---|---|
| Quad | `QuadBrush` | Can render in any pass (`render_single`, `render_slice`) |
| RichText | `RichTextBrush` | Batch-only: all text in one `prepare()` + `render()` cycle in the main pass. No `render_single()`. |
| Image | `LayerBrush` | Can render in any pass. `render()` takes an external `&mut RenderPass`. Also has `render_with_encoder()` for self-contained overlay passes. |

This means overlay text **must participate in the main RichText batch**, while overlay quads and images can render in dedicated overlay passes. The design uses a multi-phase approach:

```
Phase 1 — Main render pass:
  RichTextBrush::prepare() processes ALL RichTexts (pane content + overlay text)
  RichTextBrush::render() draws ALL text in one draw call
  → Overlay text is drawn HERE, composited with pane text

Phase 2 — Overlay quad render pass:
  QuadBrush::render_slice() draws overlay quad backgrounds
  → Quad-only overlay backgrounds are drawn HERE, on top of all text

Phase 3 — Overlay image render pass:
  LayerBrush.prepare_with_handle() uploads overlay images to atlas
  LayerBrush.render() draws overlay images
  → Overlay images are drawn HERE, on top of quads
```

**Important consequence**: Overlay text renders in the main pass (Phase 1) while overlay backgrounds render in the overlay pass (Phase 2). This means overlay quad backgrounds appear **on top of** overlay text. To achieve the expected "background behind text" visual, overlay quads that serve as text backgrounds must be injected into the main quad pass instead (via the `objects` Vec), not in the overlay quad render pass.

The actual render order for a layer with quad, text, and image is:

```
Main pass:
  1. Pane background quads
  2. Pane terminal text
  3. Overlay background quads (injected as Object::Quad into objects Vec)
  4. Overlay text (injected as Object::RichText into objects Vec)

Overlay quad pass:
  5. Overlay-only quads (borders, highlights, tints — no backing text)

Overlay image pass:
  6. Overlay images (icons, thumbnails, previews)
```

### Layer Types and Render Path Routing

Based on the multi-phase architecture, each layer's components are routed differently:

| Layer Configuration | Quad Route | Text Route | Image Route |
|---|---|---|---|
| Quad-only (focus tint, border) | Overlay quad pass | — | — |
| Text-only (floating label) | — | Main pass (`objects` Vec) | — |
| Image-only (icon, thumbnail) | — | — | Overlay image pass |
| Quad + Text (badge, tooltip) | Main pass (`objects` Vec) | Main pass (`objects` Vec) | — |
| Quad + Image (icon with bg) | Main pass (`objects` Vec) | — | Overlay image pass |
| Quad + Text + Image (full) | Main pass (`objects` Vec) | Main pass (`objects` Vec) | Overlay image pass |

This means `OverlayLayer` components are split at render time:

- Layers with **quad + text** (and optionally image): Quad and RichText go into the `objects` Vec (rendered in main pass, text on top of quad via Object ordering)
- Layers with **quad only** (no text, no image): Quad goes to the overlay quad render pass (rendered on top of everything, ideal for tints and borders)
- Layers with **text only**: RichText goes into the `objects` Vec
- Layers with **image**: Image is prepared via `LayerBrush.prepare_with_handle()` and rendered in the overlay image pass (or in the main pass if the layer also has text that needs to appear on top)

### Layer Ordering

Layers are rendered in Vec order (index 0 first, index N last). Within the main pass, Objects from lower-indexed layers are pushed before higher-indexed layers, ensuring correct z-ordering. Overlay-only quads follow the same index order in the overlay pass.

## Implementation Details

### 1. Define the `OverlayLayer` and `OverlayImage` Structs

**File:** `sugarloaf/src/sugarloaf/primitives.rs`

```rust
/// A single overlay layer that can contain a background quad, text, and/or an image.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayLayer {
    /// Background/decoration quad. None for text-only or image-only layers.
    pub quad: Option<Quad>,
    /// RichText reference (id + position). None for quad-only or image-only layers.
    pub rich_text: Option<RichText>,
    /// Image to render in this layer. None for quad-only or text-only layers.
    pub image: Option<OverlayImage>,
}

/// Image reference for an overlay layer.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayImage {
    /// Image handle — supports file path, in-memory bytes, or raw RGBA pixels.
    pub handle: image::Handle,
    /// Screen-space bounds (position + size) for the rendered image.
    pub bounds: Rectangle,
}
```

`image::Handle` is the existing image handle type used by `LayerBrush` and supports three data sources:

```rust
// From sugarloaf/src/components/layer/mod.rs (re-exported)
pub enum Data {
    Path(PathBuf),                                    // Load from filesystem
    Bytes(Cow<'static, [u8]>),                       // Decode from memory (PNG, JPEG, etc.)
    Rgba { width: u32, height: u32, pixels: Vec<u8> }, // Raw RGBA pixel data
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

### 6. Add Overlay Image Render Pass

**File:** `sugarloaf/src/sugarloaf.rs` (after the overlay quad render pass)

The existing `LayerBrush` can render into any `wgpu::RenderPass`. For overlay images, the renderer calls `prepare_with_handle()` during the prepare phase, then renders via a dedicated overlay pass:

```rust
// Overlay image preparation (before render passes, during prepare phase)
for overlay_image in &self.state.overlay_images {
    self.layer_brush.prepare_with_handle(
        &mut encoder,
        &mut self.ctx,
        &overlay_image.handle,
        &overlay_image.bounds,
    );
}

// ... later, after overlay quad pass ...

// Overlay image render pass (transparent click-through images)
if !self.state.overlay_images.is_empty() {
    let mut image_pass =
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay_images"),
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
    // Render each prepared overlay image layer
    for (i, _) in self.state.overlay_images.iter().enumerate() {
        let layer_index = self.overlay_image_layer_offset + i;
        self.layer_brush.render(layer_index, &mut image_pass, None);
    }
}
```

Key: `LayerBrush.render()` takes any `&mut wgpu::RenderPass<'a>` — it is not tied to the main pass. The `LayerBrush` pipeline already uses alpha blending (`SrcAlpha`/`OneMinusSrcAlpha`), so transparent images correctly composite over existing content.

Alternatively, `LayerBrush.render_with_encoder()` can be used for a fully self-contained overlay:

```rust
// Self-contained image overlay (creates its own LoadOp::Load pass)
self.layer_brush.render_with_encoder(layer_index, &view, &mut encoder, None);
```

### 7. Construct Overlay Layers in `Renderer::run()`

**File:** `frontends/rioterm/src/renderer/mod.rs`

The renderer builds the layer stack, then routes each layer's components to the correct render path:

```rust
// Multi-layer click-through overlay above active pane
let (overlay_objects, overlay_only_quads, overlay_images) = {
    let grid = context_manager.current_grid();
    let active_key = grid.current;
    let mut obj_vec: Vec<Object> = Vec::new();
    let mut quad_vec: Vec<Quad> = Vec::new();
    let mut img_vec: Vec<OverlayImage> = Vec::new();

    if let Some(item) = grid.inner().get(&active_key) {
        let rich_text_obj = &item.rich_text_object;
        let pane_position = rich_text_obj.position;
        let pane_size = [item.val.dimension.width, item.val.dimension.height];

        // Build layer stack
        let layers = self.build_overlay_layers(sugarloaf, pane_position, pane_size);

        // Route each layer's components to the correct render path
        for layer in &layers {
            let has_text = layer.rich_text.is_some();

            // Quad routing: main pass if layer has text, overlay pass otherwise
            if let Some(quad) = &layer.quad {
                if has_text {
                    obj_vec.push(Object::Quad(*quad));
                } else {
                    quad_vec.push(*quad);
                }
            }

            // Text routing: always main pass (batch constraint)
            if let Some(rich_text) = &layer.rich_text {
                obj_vec.push(Object::RichText(*rich_text));
            }

            // Image routing: overlay image pass
            if let Some(image) = &layer.image {
                img_vec.push(image.clone());
            }
        }
    }

    (obj_vec, quad_vec, img_vec)
};

// Inject overlay objects into the main objects Vec (for text + text-background layers)
objects.extend(overlay_objects);

// Set overlay-only quads for the dedicated overlay render pass
sugarloaf.set_overlay_only_quads(overlay_only_quads);

// Set overlay images for the dedicated image overlay render pass
sugarloaf.set_overlay_images(overlay_images);
```

### 8. Text Content Management

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

### 9. Image Content Management

Images are loaded through the existing `image::Handle` type, which supports three data sources:

```rust
// From file path (loaded and decoded lazily by raster::Cache)
let handle = image::Handle::from_path("icons/mode-badge.png");

// From in-memory bytes (PNG, JPEG, GIF, BMP, TIFF, WebP, ICO)
let png_bytes: Vec<u8> = include_bytes!("icon.png").to_vec();
let handle = image::Handle::from_memory(png_bytes);

// From raw RGBA pixels (pre-decoded, e.g., from a canvas or sixel data)
let handle = image::Handle::from_pixels(32, 32, rgba_pixels);
```

Images are cached by the `raster::Cache` inside `LayerBrush`. Once uploaded to the GPU texture atlas, subsequent frames reuse the cached atlas entry. The cache evicts unused images each frame via `trim()`, so overlay images that persist across frames are efficiently retained.

```rust
// Construct an image layer
OverlayLayer {
    quad: Some(Quad { /* background behind the image */ }),
    rich_text: None,
    image: Some(OverlayImage {
        handle: image::Handle::from_path("icons/recording.png"),
        bounds: Rectangle {
            x: badge_x + 4.0,
            y: badge_y + 4.0,
            width: 16.0,
            height: 16.0,
        },
    }),
}
```

### 10. No Input Changes Required

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
6. overlay_images     (LoadOp::Load)   — overlay image layers      ← NEW pass
7. Filters            (LoadOp::Load)   — post-processing shaders
```

### Render Path Per Layer Type

| Layer Type | Quad | Text | Image | Render Location |
|---|---|---|---|---|
| Quad-only (tint, border) | Overlay pass (5) | — | — | On top of all content |
| Text-only (label) | — | Main pass (1) | — | Mixed with terminal text |
| Image-only (icon) | — | — | Image pass (6) | On top of quads |
| Quad + Text (badge) | Main pass (1) | Main pass (1) | — | Quad behind text |
| Quad + Image (icon bg) | Main pass (1) | — | Image pass (6) | Quad behind image |
| Quad + Text + Image | Main pass (1) | Main pass (1) | Image pass (6) | All three composed |

### Why This Split Works

The split is necessary because each brush has different rendering constraints:

- **`RichTextBrush`** batches all text into a single `prepare()` + `render()` cycle within the main pass. There is no `render_single()`, so overlay text cannot be rendered in a separate pass.
- **`QuadBrush`** supports both batch rendering (`render()` in main pass) and standalone rendering (`render_single()`, `render_slice()` in overlay passes).
- **`LayerBrush`** supports rendering in any pass. `render()` takes an external `&mut RenderPass`, and `render_with_encoder()` creates its own `LoadOp::Load` pass. Its pipeline already uses alpha blending.

For layers with **Quad + Text**: Both go to the `objects` Vec. Object ordering (`Object::Quad` first, then `Object::RichText`) ensures the quad background renders behind the text, following the same pattern as the leader menu.

For layers with **Quad only**: The quad goes to the dedicated overlay pass, rendering on top of everything — ideal for tints, borders, and highlights that should cover terminal text.

For layers with **Image**: The image is prepared via `LayerBrush.prepare_with_handle()` and rendered in the overlay image pass. If the layer also has a quad background, the quad goes to the main pass (so it appears behind the image).

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

### Image Fields Per Layer

| Field | Type | Purpose |
|-------|------|---------|
| `handle` | `image::Handle` | Image data source (path, bytes, or RGBA pixels) |
| `bounds.x` | `f32` | Screen-space X position |
| `bounds.y` | `f32` | Screen-space Y position |
| `bounds.width` | `f32` | Rendered width (image is scaled to fit) |
| `bounds.height` | `f32` | Rendered height (image is scaled to fit) |

Supported image formats (via the `image` crate): PNG, JPEG, GIF, BMP, TIFF, WebP, ICO.

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
    image: None,
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
    image: None,
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
        image: None,
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
        image: None,
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
        image: None,
    },
]
```

### Example 4: Icon badge (quad + image, 1 layer)

```rust
vec![OverlayLayer {
    quad: Some(Quad {
        position: [badge_x, badge_y],
        size: [32.0, 32.0],
        color: [0.15, 0.15, 0.17, 0.9],
        border_radius: [6.0; 4],
        ..Quad::default()
    }),
    rich_text: None,
    image: Some(OverlayImage {
        handle: image::Handle::from_path("icons/recording.png"),
        bounds: Rectangle {
            x: badge_x + 4.0,
            y: badge_y + 4.0,
            width: 24.0,
            height: 24.0,
        },
    }),
}]
```

### Example 5: Status badge with icon + text (quad + image + text, 1 layer)

```rust
// Text: "RECORDING" in red
let content = sugarloaf.content();
let line = content.sel(status_rt_id);
line.clear();
line.new_line();
line.add_text("RECORDING", FragmentStyle {
    color: [1.0, 0.3, 0.3, 1.0],
    ..FragmentStyle::default()
});
line.build();

vec![OverlayLayer {
    quad: Some(Quad {
        position: [badge_x, badge_y],
        size: [130.0, 28.0],
        color: [0.12, 0.12, 0.14, 0.95],
        border_radius: [4.0; 4],
        ..Quad::default()
    }),
    rich_text: Some(RichText {
        id: status_rt_id,
        position: [badge_x + 28.0, badge_y + 6.0],  // Offset right of icon
        lines: None,
    }),
    image: Some(OverlayImage {
        handle: image::Handle::from_path("icons/record-dot.png"),
        bounds: Rectangle {
            x: badge_x + 6.0,
            y: badge_y + 6.0,
            width: 16.0,
            height: 16.0,
        },
    }),
}]
```

### Example 6: Notification toast (quad + multi-line text)

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
    image: None,
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
| Render passes | +1 for overlay quads (when non-empty), +1 for overlay images (when non-empty) |
| Draw calls per frame | +1 instanced draw for overlay quads; +1 per overlay image; text batched into existing RichText draw call |
| GPU buffer writes | +1 `write_buffer` for overlay quads; text vertices already in RichText buffer; image data in texture atlas (cached) |
| CPU overhead | `Vec<OverlayLayer>` construction + routing; text shaping only on content change; image decode only on first load |
| Memory | `sizeof(Quad) * N` on GPU for overlay quads; RichText states in Content hashmap; image pixels in texture atlas (shared with background/inline images) |
| Zero-layer cost | None — empty Vecs skip all overlay work |
| Text reshaping | Only when `BuilderState::last_update` is not `Noop` (cached across frames) |
| Image caching | `raster::Cache` retains atlas entries across frames; same `Handle` = no re-upload |

## Files Changed

| File | Changes |
|------|---------|
| `sugarloaf/src/sugarloaf/primitives.rs` | Add `OverlayLayer` and `OverlayImage` struct definitions |
| `sugarloaf/src/components/quad/mod.rs` | Add `render_slice(&[Quad])` method for multi-quad overlay rendering |
| `sugarloaf/src/sugarloaf/state.rs` | Add `overlay_layers: Vec<OverlayLayer>`, `overlay_only_quads: Vec<Quad>`, `overlay_images: Vec<OverlayImage>` fields, constructors, setters |
| `sugarloaf/src/sugarloaf.rs` | Add `set_overlay_layers()`, `set_overlay_only_quads()`, `set_overlay_images()` methods; add overlay quad render pass; add overlay image render pass with `LayerBrush` prepare + render |
| `frontends/rioterm/src/renderer/mod.rs` | Add `overlay_rich_text_ids: Vec<usize>` to `Renderer`; add `build_overlay_layers()` method; route layer components (quad/text/image) to correct render paths; inject overlay objects into `objects` Vec |
| `frontends/rioterm/src/context/grid.rs` | Add `CommandOverlayState` struct; add `command_overlays: Vec<CommandOverlayState>` to `ContextGrid`; add `open_command_overlay()` method; extend `extend_with_objects()` to render command overlay Objects |
| `frontends/rioterm/src/context/mod.rs` | Add `toggle_command_overlay()` and `handle_command_overlay_exit()` methods to `ContextManager` |
| `frontends/rioterm/src/bindings/mod.rs` | Add `Action::ToggleCommandOverlay(String)` variant; add `"togglecommandoverlay:"` prefix parsing in `from_str` |
| `frontends/rioterm/src/screen/mod.rs` | Add `Act::ToggleCommandOverlay` handler in `process_action()` |
| `rio-backend/src/config/mod.rs` | Add `CommandOverlayConfig` struct; add `command_overlays: HashMap<String, CommandOverlayConfig>` to config |

## Dependencies

None. Uses only existing infrastructure:
- `Quad` struct from `sugarloaf/src/components/quad/mod.rs`
- `RichText` struct from `sugarloaf/src/sugarloaf/primitives.rs`
- `Content` builder API from `sugarloaf/src/layout/content.rs`
- `QuadBrush` GPU buffer and instanced drawing (extended with `render_slice()`)
- `RichTextBrush` batch rendering (existing `prepare()` + `render()` cycle)
- `LayerBrush` image rendering (existing `prepare_with_handle()` + `render()` methods — no modifications needed)
- `image::Handle` for image data sources (path, bytes, RGBA pixels)
- `raster::Cache` for image caching and GPU texture atlas management
- `ContextGrid::current` and `ContextGridItem` for active pane geometry
- Standard wgpu render pass with `LoadOp::Load`
- `Context`, `Crosswords`, `Machine`, `create_pty_with_spawn` for command output overlays (existing Quick Terminal infrastructure)
- `ContextManagerConfig` and `Shell` for command override (existing config types)

## Testing

### Visual Verification

1. **Quad-only layer**: Push a single layer with visible border, no text, no image. Verify border renders on top of terminal content and is click-through.

2. **Text-only layer**: Push a single layer with RichText, no quad, no image. Verify text renders at specified position and is click-through.

3. **Image-only layer**: Push a single layer with an image (PNG from file path). Verify image renders at specified bounds with correct aspect ratio.

4. **Quad + Text layer**: Push a layer with both. Verify quad background renders behind text, both are positioned correctly.

5. **Quad + Image layer**: Push a layer with quad background and image. Verify quad renders behind the image.

6. **Quad + Text + Image layer**: Push a layer with all three. Verify quad behind text, image rendered at its own position, all click-through.

7. **Multi-layer stack**: Push 4 layers (quad-only + quad+text + image-only + quad+text+image). Verify all render in correct z-order.

8. **Split panes**: Verify overlay tracks active pane, switches on focus change, click-through works on inactive panes.

9. **Dynamic text update**: Change text content every frame (e.g., timer). Verify no flicker or memory growth.

10. **Zero layers**: Empty Vec produces no render pass and no objects — verify with wgpu debug labels.

### Click-Through Verification

With visible overlay layers (including text and image layers):
- Left-click through text/image should set cursor position / start selection
- Double-click through text/image should select a word
- Scroll through overlay should work normally
- Mouse reporting to terminal apps should work normally

### Text Rendering Verification

1. **Single-line text**: "VI MODE" in a badge — verify font, color, position
2. **Multi-line text**: Title + body in a toast — verify line spacing, wrapping
3. **Styled text**: Mixed bold/color fragments — verify `FragmentStyle` applied correctly
4. **Font size**: 12px overlay text alongside 14px terminal text — verify independent sizing
5. **Text update**: Change label from "RECORDING" to "STOPPED" — verify clean transition

### Image Rendering Verification

1. **From file path**: `Handle::from_path("test.png")` — verify image loads and renders
2. **From memory bytes**: `Handle::from_memory(png_bytes)` — verify decode and render
3. **From RGBA pixels**: `Handle::from_pixels(32, 32, pixels)` — verify raw pixel render
4. **Transparency**: PNG with alpha channel — verify alpha compositing over terminal content
5. **Scaling**: 512x512 image in a 32x32 bounds — verify correct downscaling
6. **Cache persistence**: Same image across frames — verify no re-upload (atlas cache hit)
7. **Image swap**: Change image handle between frames — verify clean transition

### Performance Verification

1. Push 0 layers → verify no render pass, no extra objects
2. Push 1 quad-only layer → verify 1 overlay render pass, 0 extra objects
3. Push 1 quad+text layer → verify 0 overlay render pass (routed to main), 2 extra objects
4. Push 1 image layer → verify 1 image overlay render pass
5. Push 5 mixed layers → verify correct routing split across all three paths
6. Rapid text updates → verify text reshaping is cached when content unchanged
7. Same image across frames → verify atlas cache hit (no re-upload)

### Command Output Overlay Verification

1. **Spawn and render**: Configure `[command-overlays.top]`, press keybinding → verify `top` output renders in overlay with correct position and size
2. **Toggle visibility**: Press keybinding again → overlay hides. Press again → overlay shows (PTY still alive, output resumed)
3. **Non-interactive click-through**: With `interactive = false`, type in underlying pane → verify keystrokes go to the underlying pane, not to the overlay command
4. **Interactive mode**: With `interactive = true` (`htop`), press keybinding → verify keyboard input is routed to `htop` (e.g., press `q` to quit)
5. **Process exit cleanup**: Spawn `git log --oneline -5` (short-lived) → verify overlay shows output, then is automatically removed when `git` exits
6. **Multiple overlays**: Open `top` overlay + `git-log` overlay simultaneously → verify both render in correct positions without conflict
7. **Fractional positioning**: Configure `x=0.6, y=0.05, width=0.35, height=0.5` → verify overlay is positioned at 60% from left, 5% from top of active pane
8. **Split pane interaction**: With split panes, verify command overlay attaches to the active pane and repositions when focus changes
9. **Focus restore**: Open interactive overlay → dismiss → verify focus returns to the original pane
10. **PTY resize**: Resize the terminal window while a command overlay is visible → verify the overlay's PTY is resized and content reflows correctly

## Relationship to Existing Overlays

| Overlay | Storage | Quad Render | Text Render | Image Render | Pass |
|---------|---------|-------------|-------------|--------------|------|
| Vi mode | `Option<Quad>` | `render_single()` | — | — | Own pass |
| Visual bell | `Option<Quad>` | `render_single()` | — | — | Own pass |
| Progress bar | `Option<Quad>` | `render_single()` | — | — | Own pass |
| Leader menu | `Vec<Object>` | Main pass | Main pass | — | Main pass |
| Quick terminal | `QuickTerminalState` | Main pass | Main pass | — | Main pass |
| Background image | `BottomLayer` | — | — | `LayerBrush` | Main pass |
| Inline images | `top_layer` | — | — | `LayerBrush` | Main pass |
| **Overlay layers** | **`Vec<OverlayLayer>`** | **Main or overlay** | **Main pass** | **`LayerBrush` overlay** | **Split routing** |
| **Command overlays** | **`Vec<CommandOverlayState>`** | **Main pass** | **Main pass** | **—** | **Main pass** |

The overlay layer system follows the leader menu's combined Quad + RichText pattern for layers that need text, uses a dedicated overlay pass for quad-only layers, and leverages the existing `LayerBrush` for image content in its own overlay pass.

## Command Output Overlay Layers

### Overview

A **command output layer** is a special overlay layer that spawns a predefined command in a real PTY and renders its live terminal output within a positioned overlay region. This reuses the same `Context` + `Crosswords` + `Machine` IO thread infrastructure that powers the Quick Terminal, but instead of spawning the user's shell, it spawns a specific command (e.g., `top`, `htop`, `git log --oneline -20`).

```
 ┌─────────────────────────────────────────────────────┐
 │  Active pane (terminal content)                     │
 │                                                     │
 │   ┌──────────────────────────────────────────────┐  │
 │   │  Command Output Overlay                      │  │
 │   │  ┌────────────────────────────────────────┐  │  │
 │   │  │ PID   USER   %CPU  %MEM   COMMAND      │  │  │
 │   │  │ 1234  wk     12.3   4.5   rio          │  │  │
 │   │  │ 5678  wk      8.1   2.1   cargo        │  │  │
 │   │  │ ...                                    │  │  │
 │   │  └────────────────────────────────────────┘  │  │
 │   │  Quad bg [0.08, 0.08, 0.10, 0.95]           │  │
 │   └──────────────────────────────────────────────┘  │
 │                                                     │
 │  Underlying pane content visible around overlay     │
 └─────────────────────────────────────────────────────┘

 Trigger: keyboard shortcut (e.g., leader + t)
 Dismiss: same shortcut (toggle) or process exit
```

### Relationship to Quick Terminal

The Quick Terminal and command output overlays share the same core machinery but differ in purpose and behavior:

| Aspect | Quick Terminal | Command Output Overlay |
|---|---|---|
| Purpose | General-purpose overlay shell | Display specific command output |
| Shell program | User's configured shell (`config.shell`) | Predefined command (e.g., `"top"`) |
| Trigger | `Action::ToggleQuickTerminal` | `Action::ToggleCommandOverlay(id)` (new) |
| Interactivity | Always interactive (receives keyboard input) | Click-through by default; optionally interactive |
| Lifetime | Persists until dismissed or shell exits | Dismisses on toggle or process exit |
| Position/size | Full-width bottom overlay | Configurable position and size within active pane |
| Storage | `ContextGrid::quick_terminal` (`QuickTerminalState`) | `ContextGrid::command_overlays` (`Vec<CommandOverlayState>`) (new) |
| Count | One per grid | Multiple simultaneous overlays |

### The `CommandOverlayState` Struct

```rust
/// State for a command output overlay.
/// Similar to QuickTerminalState but for a specific predefined command.
pub struct CommandOverlayState<T: EventListener> {
    /// The command overlay's context (PTY + terminal grid + IO thread)
    pub item: ContextGridItem<T>,
    /// Whether the overlay is currently visible
    pub visible: bool,
    /// The key of the pane that was focused before (for focus restore)
    pub saved_focus: usize,
    /// Configuration identifier (matches config key, e.g., "top", "git-log")
    pub config_id: String,
    /// Whether this overlay accepts keyboard input (default: false = click-through)
    pub interactive: bool,
    /// Overlay position relative to active pane [x, y] (screen-space)
    pub position: [f32; 2],
    /// Overlay size [width, height] (screen-space)
    pub size: [f32; 2],
}
```

Storage on `ContextGrid`:

```rust
pub struct ContextGrid<T: EventListener> {
    // ... existing fields ...
    pub quick_terminal: Option<QuickTerminalState<T>>,
    pub command_overlays: Vec<CommandOverlayState<T>>,  // NEW
}
```

### Configuration

Command overlays are defined in the Rio config file. Each entry specifies a command, optional args, and display properties:

```toml
# rio.toml

[command-overlays.top]
program = "top"
args = ["-o", "cpu"]
# Position and size as fractions of the active pane (0.0 - 1.0)
x = 0.05          # 5% from left edge of pane
y = 0.05          # 5% from top edge of pane
width = 0.9       # 90% of pane width
height = 0.9      # 90% of pane height
interactive = false  # Click-through (no keyboard input to the command)

[command-overlays.htop]
program = "htop"
interactive = true   # Receives keyboard input when visible
x = 0.0
y = 0.0
width = 1.0
height = 1.0

[command-overlays.git-log]
program = "git"
args = ["log", "--oneline", "-20", "--color=always"]
x = 0.6
y = 0.05
width = 0.35
height = 0.5
interactive = false
```

Config struct:

```rust
/// Configuration for a predefined command overlay.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandOverlayConfig {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_x")]
    pub x: f32,
    #[serde(default = "default_y")]
    pub y: f32,
    #[serde(default = "default_width")]
    pub width: f32,
    #[serde(default = "default_height")]
    pub height: f32,
    #[serde(default)]
    pub interactive: bool,
}

fn default_x() -> f32 { 0.05 }
fn default_y() -> f32 { 0.05 }
fn default_width() -> f32 { 0.9 }
fn default_height() -> f32 { 0.9 }
```

### New Action Variant

```rust
// In frontends/rioterm/src/bindings/mod.rs
pub enum Action {
    // ... existing variants ...

    /// Toggle a predefined command overlay by its config ID.
    /// The String matches a key in [command-overlays.*] config section.
    ToggleCommandOverlay(String),
}
```

Config binding example:

```toml
[keyboard]
# Leader menu approach: leader -> "t" toggles the "top" overlay
# Or direct keybinding:
[keyboard.bindings]
keys = "super+shift+t"
action = "togglecommandoverlay:top"
```

Parsing in `Action::from_str`:

```rust
// In frontends/rioterm/src/bindings/mod.rs
if let Some(id) = s.strip_prefix("togglecommandoverlay:") {
    return Some(Action::ToggleCommandOverlay(id.to_string()));
}
```

### Lifecycle

```
 User presses keybinding (e.g., super+shift+t)
   │
   ▼
 Screen::process_action(Action::ToggleCommandOverlay("top"))
   │
   ├─ Overlay exists + visible? ──► Hide: set visible=false, restore focus
   │
   ├─ Overlay exists + hidden?  ──► Show: set visible=true, save focus
   │
   └─ Overlay doesn't exist?   ──► Create:
        │
        ▼
      1. Look up CommandOverlayConfig for "top"
      2. Clone ContextManagerConfig, override shell:
         cloned_config.shell = Shell {
             program: "top".into(),
             args: vec!["-o", "cpu"],
         };
      3. Compute overlay dimensions from config fractions + active pane bounds
      4. ContextManager::create_context(... cloned_config ...) → Context with PTY
         - PTY spawns `top -o cpu`
         - Machine IO thread reads PTY output, parses ANSI, updates Crosswords grid
      5. grid.open_command_overlay(context, config_id, config) → CommandOverlayState
      6. If interactive: focus the overlay (self.current = overlay route_id)
         If not interactive: keep focus on the underlying pane

 Process exits (RioEvent::Exit for the overlay's route_id):
   │
   ▼
 ContextManager detects exit for a command overlay route → remove overlay, restore focus
```

### Spawning the Command

The command is spawned through the same `create_context` path as Quick Terminal, with the `ContextManagerConfig.shell` field overridden:

```rust
// In frontends/rioterm/src/context/mod.rs
impl<T: EventListener> ContextManager<T> {
    pub fn toggle_command_overlay(
        &mut self,
        config_id: &str,
        rich_text_id: usize,
        overlay_config: &CommandOverlayConfig,
    ) {
        let grid = &mut self.contexts[self.current_index];

        // Check if this overlay already exists
        if let Some(idx) = grid.command_overlays.iter().position(|o| o.config_id == config_id) {
            let overlay = &mut grid.command_overlays[idx];
            if overlay.visible {
                // Hide
                overlay.visible = false;
                self.current_route = grid.current().route_id;
                if !overlay.interactive {
                    // Focus was never moved, nothing to restore
                } else {
                    grid.current = overlay.saved_focus;
                    self.current_route = grid.current().route_id;
                }
            } else {
                // Show
                overlay.visible = true;
                overlay.saved_focus = grid.current;
                if overlay.interactive {
                    grid.current = overlay.item.val.route_id;
                    self.current_route = overlay.item.val.route_id;
                }
            }
            return;
        }

        // Create new overlay — override shell to the predefined command
        let mut cloned_config = self.config.clone();
        cloned_config.shell = Shell {
            program: overlay_config.program.clone(),
            args: overlay_config.args.clone(),
        };

        // Compute overlay dimensions from fractional config
        let current = grid.current();
        let cursor = current.cursor_from_ref();
        let pane_dim = current.dimension;

        let overlay_width = pane_dim.width * overlay_config.width;
        let overlay_height = pane_dim.height * overlay_config.height;
        let dimension = ContextDimension::build(
            overlay_width,
            overlay_height,
            pane_dim.dimension,
            pane_dim.line_height,
            grid.margin,
        );

        match ContextManager::create_context(
            (&cursor, current.renderable_content.has_blinking_enabled),
            self.event_proxy.clone(),
            self.window_id,
            rich_text_id,
            dimension,
            &cloned_config,
        ) {
            Ok(new_context) => {
                let new_route_id = new_context.route_id;
                grid.open_command_overlay(
                    new_context,
                    config_id.to_string(),
                    overlay_config,
                );
                if overlay_config.interactive {
                    self.current_route = new_route_id;
                }
            }
            Err(e) => {
                tracing::error!("failed to create command overlay '{config_id}': {e}");
            }
        }
    }
}
```

### Opening the Overlay on `ContextGrid`

```rust
// In frontends/rioterm/src/context/grid.rs
impl<T: EventListener> ContextGrid<T> {
    pub fn open_command_overlay(
        &mut self,
        context: Context<T>,
        config_id: String,
        config: &CommandOverlayConfig,
    ) {
        let saved_focus = self.current;
        let mut item = ContextGridItem::new(context);

        // Compute screen-space position from fractional config + active pane bounds
        let pane_item = self.inner.get(&self.current).unwrap();
        let pane_pos = pane_item.position();
        let pane_dim = &pane_item.val.dimension;
        let scale = pane_dim.dimension.scale;

        let overlay_x = pane_pos[0] + (config.x * pane_dim.width / scale);
        let overlay_y = pane_pos[1] + (config.y * pane_dim.height / scale);
        let overlay_width = config.width * pane_dim.width;
        let overlay_height = config.height * pane_dim.height;

        item.val.dimension.update_width(overlay_width);
        item.val.dimension.update_height(overlay_height);
        item.set_position([overlay_x, overlay_y]);

        // Resize PTY to match overlay dimensions
        let mut terminal = item.val.terminal.lock();
        terminal.resize::<ContextDimension>(item.val.dimension);
        drop(terminal);
        let winsize = crate::renderer::utils::terminal_dimensions(&item.val.dimension);
        let _ = item.val.messenger.send_resize(winsize);

        if config.interactive {
            self.current = item.val.route_id;
        }

        self.command_overlays.push(CommandOverlayState {
            item,
            visible: true,
            saved_focus,
            config_id,
            interactive: config.interactive,
            position: [overlay_x, overlay_y],
            size: [overlay_width, overlay_height],
        });
    }
}
```

### Rendering Command Output

Command output overlays are rendered the same way as the Quick Terminal: the overlay's `ContextGridItem` contains a `rich_text_object` (`Object::RichText`) that references the overlay's `Crosswords` grid. The `Machine` IO thread continuously reads PTY output and updates the `Crosswords` grid with parsed ANSI content, which the `RichTextBrush` renders each frame.

```rust
// In frontends/rioterm/src/context/grid.rs — extend_with_objects()
// After quick terminal rendering:

// Add command output overlays if visible
for overlay in &self.command_overlays {
    if !overlay.visible {
        continue;
    }

    let scale = overlay.item.val.dimension.dimension.scale;

    // Opaque background quad
    target.push(Object::Quad(Quad {
        position: overlay.position.map(|v| v),
        color: background_color,
        size: [
            overlay.size[0] / scale,
            overlay.size[1] / scale,
        ],
        border_radius: [4.0; 4],
        ..Quad::default()
    }));

    // Optional border
    target.push(Object::Quad(Quad {
        position: overlay.position,
        color: [0.0, 0.0, 0.0, 0.0],
        size: [overlay.size[0] / scale, overlay.size[1] / scale],
        border_color: [0.3, 0.3, 0.35, 0.8],
        border_width: 1.0,
        border_radius: [4.0; 4],
        ..Quad::default()
    }));

    // Terminal content (RichText from the overlay's Crosswords grid)
    target.push(overlay.item.rich_text_object.clone());
}
```

### Input Routing

For **non-interactive** overlays (the default), input routing is unchanged — the overlay is purely visual, and keyboard/mouse events continue to flow to the underlying pane. The overlay's `route_id` is never set as `grid.current`, so `ContextGrid::current()` returns the underlying pane.

For **interactive** overlays, `grid.current` is set to the overlay's `route_id` when the overlay is shown. This means:
- `ContextGrid::current()` returns the overlay's `Context`
- `Screen::input_character()` writes to the overlay's PTY (e.g., `q` to quit `htop`)
- Mouse events are dispatched to the overlay's `Context` dimensions

When the overlay is dismissed (toggle or process exit), `grid.current` is restored to `saved_focus`.

```rust
// In frontends/rioterm/src/screen/mod.rs
Act::ToggleCommandOverlay(ref config_id) => {
    if let Some(overlay_config) = self.config.command_overlays.get(config_id) {
        let rich_text_id = self.sugarloaf.create_rich_text();
        self.context_manager.toggle_command_overlay(
            config_id,
            rich_text_id,
            overlay_config,
        );
    } else {
        tracing::warn!("unknown command overlay: {config_id}");
    }
}
```

### Process Exit Handling

When the spawned command exits, the `Machine` IO thread sends `RioEvent::Exit` with the overlay's `route_id`. The `ContextManager` must detect that this route belongs to a command overlay (not a normal pane or Quick Terminal) and clean it up:

```rust
// In frontends/rioterm/src/context/mod.rs
// When handling RioEvent::Exit for a route_id:
pub fn handle_command_overlay_exit(&mut self, route_id: usize) -> bool {
    let grid = &mut self.contexts[self.current_index];
    if let Some(idx) = grid.command_overlays.iter().position(
        |o| o.item.val.route_id == route_id
    ) {
        let overlay = grid.command_overlays.remove(idx);
        if overlay.interactive && grid.current == route_id {
            grid.current = overlay.saved_focus;
        }
        self.current_route = grid.current().route_id;
        return true;
    }
    false
}
```

### Architecture Diagram (Command Output Overlay)

```
 User presses keybinding
   │
   ▼
 Screen::process_action(ToggleCommandOverlay("top"))
   │
   ▼
 ContextManager::toggle_command_overlay("top", rich_text_id, config)
   │
   ├── Override config.shell = Shell { program: "top", args: ["-o", "cpu"] }
   │
   ▼
 ContextManager::create_context(... cloned_config ...)
   │
   ├── Crosswords::new(dimension)                    ← Terminal grid (rows × cols)
   ├── create_pty_with_spawn("top", ["-o", "cpu"])   ← Real PTY running `top`
   ├── Machine::new(terminal, pty)                    ← IO thread: reads PTY → parses ANSI → updates Crosswords
   │
   ▼
 ContextGrid::open_command_overlay(context, "top", config)
   │
   ├── Position overlay within active pane bounds
   ├── Resize PTY to overlay dimensions
   ├── Store as CommandOverlayState { visible: true, interactive: false }
   │
   ▼
 Render loop (each frame):
   │
   ├── Crosswords grid is updated by Machine IO thread (live `top` output)
   ├── extend_with_objects() appends:
   │     Object::Quad (background)
   │     Object::Quad (border)
   │     Object::RichText (overlay terminal content)
   │
   ▼
 Main render pass: background quad + border + RichText (terminal output)

 Process exits (`top` is killed or finishes):
   │
   ▼
 RioEvent::Exit(route_id) → handle_command_overlay_exit() → remove overlay, restore focus
```

### Command Output Overlay Examples

#### Example 7: System monitor overlay (non-interactive, triggered by keybinding)

```toml
# Config
[command-overlays.top]
program = "top"
args = ["-o", "cpu"]
x = 0.05
y = 0.05
width = 0.9
height = 0.9
interactive = false

[keyboard.bindings]
keys = "super+shift+t"
action = "togglecommandoverlay:top"
```

Pressing `super+shift+t` spawns `top -o cpu` in a PTY, renders its live output in a 90%×90% overlay centered in the active pane. The overlay is click-through — the user cannot type into `top`, but can still interact with the underlying pane. Pressing the same keybinding again hides the overlay (PTY remains alive). When `top` exits, the overlay is automatically removed.

#### Example 8: Interactive htop overlay (receives keyboard input)

```toml
[command-overlays.htop]
program = "htop"
interactive = true
x = 0.0
y = 0.0
width = 1.0
height = 1.0

[keyboard.bindings]
keys = "super+shift+h"
action = "togglecommandoverlay:htop"
```

Pressing `super+shift+h` spawns `htop` in a full-pane overlay. Because `interactive = true`, keyboard input is routed to `htop`'s PTY — the user can press `q` to quit, use arrow keys to navigate, etc. Focus returns to the underlying pane on dismiss.

#### Example 9: Git log sidebar (non-interactive, partial pane)

```toml
[command-overlays.git-log]
program = "git"
args = ["log", "--oneline", "-20", "--color=always"]
x = 0.6
y = 0.05
width = 0.35
height = 0.5

[keyboard.bindings]
keys = "super+shift+g"
action = "togglecommandoverlay:git-log"
```

Pressing `super+shift+g` spawns `git log` in a small overlay on the right side of the pane. The command exits naturally after producing output (it's not interactive like `top`), and the overlay remains visible showing the static output until the user toggles it off or the process exits.

## Future Enhancements

1. **Dedicated overlay text pass**: Add `RichTextBrush::prepare_overlay()` + `render_overlay()` methods to support rendering overlay text in a separate pass, eliminating the main-pass routing split
2. **Named layers**: Layer identifiers for targeted updates without rebuilding the full Vec
3. **Layer animation**: Per-layer opacity/position animation with easing curves (including image fade-in/fade-out)
4. **Per-pane layer stacks**: Overlay layers on inactive panes (notification badges, status indicators)
5. **Configurable presets**: User-configurable overlay styles via config (focus tint color, badge position, font size)
6. **SVG support**: Add SVG rendering to `OverlayImage` for resolution-independent icons and vector graphics
7. **Image animation**: Support animated images (GIF, APNG) in overlay layers with frame cycling
8. **Async image loading**: Load images from URL or network in background threads, displaying a placeholder until loaded
9. **Command overlay auto-refresh**: Periodically re-run short-lived commands (e.g., `git status`) and update the overlay with fresh output
10. **Command overlay opacity**: Configurable background opacity per command overlay (e.g., semi-transparent `top` overlay)
11. **Leader menu integration**: Trigger command overlays from leader menu items (e.g., leader → "t" → top overlay)
12. **Command overlay scroll**: Enable scrollback in non-interactive command overlays via mouse wheel or configurable keybinding
