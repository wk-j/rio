# CR-010: 2D Layout Distortion — Post-Processing Shader Effects

**Status:** Proposed
**Date:** 2026-02-21
**Author:** wk

## Summary

Add a configurable post-processing distortion pass to the sugarloaf rendering
pipeline that applies non-linear 2D transformations (barrel distortion, wave,
perspective tilt, CRT curvature) to the final rendered frame. The distortion
is implemented as a full-screen fragment shader that runs after all geometry
passes but before the existing FiltersBrush, sampling the rendered terminal
image with mathematically distorted UV coordinates. A new `[distortion]`
config section controls the effect type, strength, center point, and
animation parameters, with hot-reload support.

## Motivation

1. **Visual customization**: Users want to personalize their terminal beyond
   colors and fonts. Distortion effects like CRT curvature, barrel lens, and
   subtle perspective tilt create distinctive visual identities.

2. **Retro/CRT aesthetics**: CRT-style barrel distortion pairs naturally with
   the existing RetroArch filter support (`renderer.filters`). A native
   distortion pass provides a lightweight alternative to full CRT shader
   presets that is easier to configure and cheaper to render.

3. **Complementary to existing filters**: The `FiltersBrush` already supports
   RetroArch `.slangp` presets for post-processing. A dedicated distortion
   pass slots cleanly before that pipeline — distort the layout first, then
   apply scanlines/bloom/color grading on top.

4. **Clean injection point**: The rendering pipeline already separates
   geometry passes (quad, rich_text, layer) from post-processing
   (FiltersBrush). A new distortion pass fits naturally between them with
   no changes to geometry shaders or SDF calculations.

5. **Animated effects**: Time-based distortion (wave, breathing) adds subtle
   motion to the terminal. The existing overlay render loop already requests
   continuous redraws for progress bar and cursor glow animations, so the
   infrastructure for per-frame updates exists.

## Architecture

### Render Pipeline Placement

```
Sugarloaf::render()
┌─────────────────────────────────────────────────────────┐
│ 1. Main render pass (LoadOp::Clear)                     │
│    ├── LayerBrush (background images)                   │
│    ├── QuadBrush (cell backgrounds, borders)            │
│    └── RichTextBrush (text glyphs)                      │
│                                                         │
│ 2. Overlay render pass (LoadOp::Load)                   │
│    ├── Cursor glow layers                               │
│    ├── Vi mode / visual bell / progress bar             │
│    └── QuadBrush::render_batch()                        │
│                                                         │
│ 3. DistortionBrush::render()                    ← NEW   │
│    ├── Copy frame.texture → src_copy                    │
│    ├── Full-screen triangle draw with distortion shader │
│    └── Output → frame.texture                           │
│                                                         │
│ 4. FiltersBrush::render() (RetroArch presets)           │
│                                                         │
│ 5. Submit + Present                                     │
└─────────────────────────────────────────────────────────┘
```

### Data Flow

```
Config TOML              rio-backend                  Sugarloaf
[distortion]          DistortionConfig              DistortionBrush
 type = "barrel"  ──►  parsed by serde  ──►  update_distortion()
 strength = 0.3        into Config.distortion       │
 center = [0.5, 0.5]                                ▼
 animated = false                           Creates/updates:
                                            - uniform buffer
                                            - wgpu pipeline
                                            - bind group
                                                    │
                                                    ▼
                                            render():
                                            1. Copy src texture
                                            2. Bind distortion params
                                            3. Draw full-screen triangle
                                            4. Fragment shader samples
                                               with distorted UVs
```

### State Management

```
Sugarloaf<'a>
  ├── quad_brush: QuadBrush
  ├── rich_text_brush: RichTextBrush
  ├── layer_brush: LayerBrush
  ├── filters_brush: Option<FiltersBrush>
  ├── distortion_brush: Option<DistortionBrush>   ← NEW
  └── state: SugarState
```

## Implementation Details

### 1. Config Types — `rio-backend/src/config/distortion.rs`

New config module for distortion parameters with serde deserialization:

```rust
// rio-backend/src/config/distortion.rs

use serde::Deserialize;

/// Distortion effect type applied to the rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DistortionType {
    #[default]
    None,
    /// Barrel/pincushion distortion (CRT curvature)
    Barrel,
    /// Perspective tilt (vanishing point effect)
    Perspective,
    /// Sinusoidal wave distortion (animated)
    Wave,
    /// Fisheye lens effect
    Fisheye,
}

/// Configuration for the `[distortion]` TOML section.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DistortionConfig {
    /// Effect type (default: none)
    #[serde(default)]
    pub effect: DistortionType,

    /// Effect strength, 0.0 = no distortion, 1.0 = maximum.
    /// Negative values invert the effect (e.g., pincushion
    /// instead of barrel). Default: 0.3
    #[serde(default = "default_strength")]
    pub strength: f32,

    /// Distortion center point in normalized coordinates
    /// (0.0–1.0). Default: [0.5, 0.5] (screen center).
    #[serde(default = "default_center")]
    pub center: [f32; 2],

    /// Enable time-based animation. Only meaningful for
    /// wave effect. Default: false
    #[serde(default)]
    pub animated: bool,

    /// Animation speed multiplier. Default: 1.0
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_strength() -> f32 {
    0.3
}

fn default_center() -> [f32; 2] {
    [0.5, 0.5]
}

fn default_speed() -> f32 {
    1.0
}

impl Default for DistortionConfig {
    fn default() -> Self {
        Self {
            effect: DistortionType::None,
            strength: default_strength(),
            center: default_center(),
            animated: false,
            speed: default_speed(),
        }
    }
}
```

### 2. Config Integration — `rio-backend/src/config/mod.rs`

Register the new module and add the field to `Config`:

```rust
// rio-backend/src/config/mod.rs

pub mod distortion;

use crate::config::distortion::DistortionConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    // ... existing fields ...
    pub renderer: Renderer,
    #[serde(default)]
    pub distortion: DistortionConfig,  // NEW
    // ... remaining fields ...
}
```

### 3. GPU Uniform Struct — `sugarloaf/src/components/distortion/mod.rs`

A `repr(C)` struct matching the shader's uniform buffer layout:

```rust
// sugarloaf/src/components/distortion/mod.rs

use bytemuck::{Pod, Zeroable};

/// Distortion type constants matching the shader.
pub const DISTORTION_NONE: u32 = 0;
pub const DISTORTION_BARREL: u32 = 1;
pub const DISTORTION_PERSPECTIVE: u32 = 2;
pub const DISTORTION_WAVE: u32 = 3;
pub const DISTORTION_FISHEYE: u32 = 4;

/// GPU-side distortion parameters. Uploaded as a uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct DistortionParams {
    /// 0=none, 1=barrel, 2=perspective, 3=wave, 4=fisheye
    pub distortion_type: u32,
    /// Effect magnitude (can be negative for inverse)
    pub strength: f32,
    /// Normalized center point [x, y]
    pub center: [f32; 2],
    /// Elapsed time in seconds (for animated effects)
    pub time: f32,
    /// Animation speed multiplier
    pub speed: f32,
    pub _padding: [f32; 2],
}
```

### 4. DistortionBrush — `sugarloaf/src/components/distortion/mod.rs`

The brush owns the wgpu pipeline, uniform buffer, and bind group:

```rust
// sugarloaf/src/components/distortion/mod.rs

pub struct DistortionBrush {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    params_bind_group_layout: wgpu::BindGroupLayout,
    current_params: DistortionParams,
}

impl DistortionBrush {
    pub fn new(ctx: &Context) -> Self {
        let shader = ctx.device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("sugarloaf::distortion shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("distortion.wgsl").into(),
                ),
            },
        );

        // Bind group 0: source texture + sampler
        let bind_group_layout = ctx.device
            .create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some(
                        "sugarloaf::distortion texture layout",
                    ),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type:
                                    wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
                                view_dimension:
                                    wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(
                                wgpu::SamplerBindingType::Filtering,
                            ),
                            count: None,
                        },
                    ],
                },
            );

        // Bind group 1: distortion params uniform
        let params_bind_group_layout = ctx.device
            .create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some(
                        "sugarloaf::distortion params layout",
                    ),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility:
                                wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                },
            );

        let params = DistortionParams {
            distortion_type: DISTORTION_NONE,
            strength: 0.0,
            center: [0.5, 0.5],
            time: 0.0,
            speed: 1.0,
            _padding: [0.0; 2],
        };

        let params_buffer = ctx.device.create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("sugarloaf::distortion params"),
                size: std::mem::size_of::<DistortionParams>()
                    as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::UNIFORM
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );

        let params_bind_group = ctx.device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some(
                    "sugarloaf::distortion params bind group",
                ),
                layout: &params_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                }],
            },
        );

        let sampler = ctx.device.create_sampler(
            &wgpu::SamplerDescriptor {
                label: Some("sugarloaf::distortion sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            },
        );

        let pipeline_layout = ctx.device
            .create_pipeline_layout(
                &wgpu::PipelineLayoutDescriptor {
                    label: Some(
                        "sugarloaf::distortion pipeline layout",
                    ),
                    bind_group_layouts: &[
                        &bind_group_layout,
                        &params_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                },
            );

        let pipeline = ctx.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: Some("sugarloaf::distortion pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options:
                        wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(
                        wgpu::ColorTargetState {
                            format: ctx.format,
                            blend: Some(
                                wgpu::BlendState::REPLACE,
                            ),
                            write_mask:
                                wgpu::ColorWrites::ALL,
                        },
                    )],
                    compilation_options:
                        wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology:
                        wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            },
        );

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            params_buffer,
            params_bind_group,
            params_bind_group_layout,
            current_params: params,
        }
    }

    /// Update distortion parameters. Called when config
    /// changes or every frame for animated effects.
    pub fn update_params(
        &mut self,
        queue: &wgpu::Queue,
        params: DistortionParams,
    ) {
        self.current_params = params;
        queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::bytes_of(&params),
        );
    }

    /// Render the distortion pass. Copies src_texture,
    /// then draws a full-screen triangle with distorted UV
    /// sampling back to dst_texture.
    pub fn render(
        &self,
        ctx: &Context,
        encoder: &mut wgpu::CommandEncoder,
        src_texture: &wgpu::Texture,
        dst_texture: &wgpu::Texture,
    ) {
        if self.current_params.distortion_type
            == DISTORTION_NONE
        {
            return;
        }

        // Copy src to a temporary texture (can't read and
        // write the same texture in one pass)
        let src_copy = ctx.device.create_texture(
            &wgpu::TextureDescriptor {
                label: Some(
                    "sugarloaf::distortion src copy",
                ),
                size: src_texture.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ctx.format,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );

        encoder.copy_texture_to_texture(
            src_texture.as_image_copy(),
            src_copy.as_image_copy(),
            src_texture.size(),
        );

        let src_view = src_copy.create_view(
            &wgpu::TextureViewDescriptor::default(),
        );
        let dst_view = dst_texture.create_view(
            &wgpu::TextureViewDescriptor::default(),
        );

        let texture_bind_group =
            ctx.device.create_bind_group(
                &wgpu::BindGroupDescriptor {
                    label: Some(
                        "sugarloaf::distortion tex bg",
                    ),
                    layout: &self.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource:
                                wgpu::BindingResource::TextureView(
                                    &src_view,
                                ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource:
                                wgpu::BindingResource::Sampler(
                                    &self.sampler,
                                ),
                        },
                    ],
                },
            );

        let mut pass =
            encoder.begin_render_pass(
                &wgpu::RenderPassDescriptor {
                    label: Some(
                        "sugarloaf::distortion pass",
                    ),
                    color_attachments: &[Some(
                        wgpu::RenderPassColorAttachment {
                            view: &dst_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        },
                    )],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                },
            );

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &texture_bind_group, &[]);
        pass.set_bind_group(
            1,
            &self.params_bind_group,
            &[],
        );
        pass.draw(0..3, 0..1); // full-screen triangle
    }
}
```

### 5. Distortion Shader — `sugarloaf/src/components/distortion/distortion.wgsl`

The WGSL shader implements all distortion types in a single fragment shader,
selected by the `distortion_type` uniform:

```wgsl
// sugarloaf/src/components/distortion/distortion.wgsl

struct DistortionParams {
    distortion_type: u32,
    strength: f32,
    center: vec2<f32>,
    time: f32,
    speed: f32,
    _padding: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;
@group(1) @binding(0) var<uniform> params: DistortionParams;

// Full-screen triangle (same pattern as blit.wgsl)
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32
) -> VertexOutput {
    var out: VertexOutput;
    let x = i32(vertex_index) / 2;
    let y = i32(vertex_index) & 1;
    let tc = vec2<f32>(f32(x) * 2.0, f32(y) * 2.0);
    out.position = vec4<f32>(
        tc.x * 2.0 - 1.0,
        1.0 - tc.y * 2.0,
        0.0,
        1.0,
    );
    out.tex_coords = tc;
    return out;
}

/// Barrel / pincushion distortion.
/// Positive strength = barrel (CRT bulge),
/// negative = pincushion (inward pinch).
fn barrel_distort(
    uv: vec2<f32>,
    center: vec2<f32>,
    k: f32,
) -> vec2<f32> {
    let d = uv - center;
    let r2 = dot(d, d);
    let scale = 1.0 + k * r2;
    return center + d * scale;
}

/// Perspective tilt around the center point.
/// Simulates tilting the screen backward (positive strength)
/// or forward (negative).
fn perspective_distort(
    uv: vec2<f32>,
    center: vec2<f32>,
    k: f32,
) -> vec2<f32> {
    let d = uv - center;
    // Apply stronger displacement near the top,
    // less near the bottom
    let perspective_y = 1.0 + k * d.y;
    return vec2<f32>(
        center.x + d.x / perspective_y,
        center.y + d.y,
    );
}

/// Sinusoidal wave distortion (animated).
fn wave_distort(
    uv: vec2<f32>,
    k: f32,
    time: f32,
    speed: f32,
) -> vec2<f32> {
    let t = time * speed;
    let wave_x = sin(uv.y * 10.0 + t * 3.0) * k * 0.02;
    let wave_y = cos(uv.x * 10.0 + t * 2.0) * k * 0.02;
    return uv + vec2<f32>(wave_x, wave_y);
}

/// Fisheye lens distortion.
fn fisheye_distort(
    uv: vec2<f32>,
    center: vec2<f32>,
    k: f32,
) -> vec2<f32> {
    let d = uv - center;
    let r = length(d);
    let theta = atan2(d.y, d.x);
    let mapped_r = pow(r, 1.0 + k) / pow(0.5, k);
    return center + vec2<f32>(
        cos(theta) * mapped_r,
        sin(theta) * mapped_r,
    );
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var uv = input.tex_coords;

    // 1 = barrel, 2 = perspective, 3 = wave, 4 = fisheye
    switch params.distortion_type {
        case 1u: {
            uv = barrel_distort(
                uv, params.center, params.strength,
            );
        }
        case 2u: {
            uv = perspective_distort(
                uv, params.center, params.strength,
            );
        }
        case 3u: {
            uv = wave_distort(
                uv, params.strength, params.time,
                params.speed,
            );
        }
        case 4u: {
            uv = fisheye_distort(
                uv, params.center, params.strength,
            );
        }
        default: {}
    }

    // Clamp to valid UV range; out-of-bounds samples
    // return black (transparent)
    if uv.x < 0.0 || uv.x > 1.0
        || uv.y < 0.0 || uv.y > 1.0
    {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    return textureSample(src_texture, tex_sampler, uv);
}
```

### 6. Module Registration — `sugarloaf/src/components/mod.rs`

Add the new distortion module:

```rust
// sugarloaf/src/components/mod.rs

pub mod core;
pub mod distortion;  // NEW
pub mod filters;
pub mod layer;
pub mod quad;
pub mod rich_text;
```

### 7. Sugarloaf Integration — `sugarloaf/src/sugarloaf.rs`

Add the `distortion_brush` field and integrate into the render pipeline:

```rust
// sugarloaf/src/sugarloaf.rs

use crate::components::distortion::{
    DistortionBrush, DistortionParams,
};

pub struct Sugarloaf<'a> {
    pub ctx: Context<'a>,
    quad_brush: QuadBrush,
    rich_text_brush: RichTextBrush,
    layer_brush: LayerBrush,
    state: state::SugarState,
    pub background_color: Option<wgpu::Color>,
    pub background_image: Option<ImageProperties>,
    pub graphics: Graphics,
    filters_brush: Option<FiltersBrush>,
    distortion_brush: Option<DistortionBrush>,  // NEW
}
```

New public API methods:

```rust
// sugarloaf/src/sugarloaf.rs

/// Enable or disable distortion with the given parameters.
/// Pass distortion_type = 0 to disable.
pub fn update_distortion(
    &mut self,
    params: DistortionParams,
) {
    if params.distortion_type == 0 {
        self.distortion_brush = None;
        return;
    }

    if self.distortion_brush.is_none() {
        self.distortion_brush =
            Some(DistortionBrush::new(&self.ctx));
    }

    if let Some(ref mut brush) = self.distortion_brush {
        brush.update_params(&self.ctx.queue, params);
    }
}
```

In the `render()` method, insert between the overlay pass and FiltersBrush
(after line ~507 in the current code, before the existing filters call):

```rust
// sugarloaf/src/sugarloaf.rs — render()

// --- Distortion pass (after overlays, before filters) ---
if let Some(ref distortion_brush) = self.distortion_brush {
    distortion_brush.render(
        &self.ctx,
        &mut encoder,
        &frame.texture,
        &frame.texture,
    );
}

// --- Existing filters pass ---
if let Some(ref mut filters_brush) = self.filters_brush {
    filters_brush.render(
        &self.ctx,
        &mut encoder,
        &frame.texture,
        &frame.texture,
    );
}
```

### 8. Renderer Integration — `frontends/rioterm/src/renderer/mod.rs`

Update the renderer to pass distortion params to sugarloaf each frame. For
animated effects, the time value must be updated every frame:

```rust
// frontends/rioterm/src/renderer/mod.rs — inside run()

use sugarloaf::components::distortion::{
    DistortionParams, DISTORTION_BARREL,
    DISTORTION_FISHEYE, DISTORTION_NONE,
    DISTORTION_PERSPECTIVE, DISTORTION_WAVE,
};

// Convert config enum to GPU constant
let distortion_type = match config.distortion.effect {
    DistortionType::None => DISTORTION_NONE,
    DistortionType::Barrel => DISTORTION_BARREL,
    DistortionType::Perspective => DISTORTION_PERSPECTIVE,
    DistortionType::Wave => DISTORTION_WAVE,
    DistortionType::Fisheye => DISTORTION_FISHEYE,
};

let time = if config.distortion.animated {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f32()
} else {
    0.0
};

sugarloaf.update_distortion(DistortionParams {
    distortion_type,
    strength: config.distortion.strength,
    center: config.distortion.center,
    time,
    speed: config.distortion.speed,
    _padding: [0.0; 2],
});
```

### 9. Config Hot-Reload — `frontends/rioterm/src/screen/mod.rs`

The distortion config is automatically picked up on reload because
`config.distortion` is read every frame in the renderer (step 8). No
explicit reload handling is needed beyond the existing config update path
that already replaces `self.config`.

## Files Changed

| File | Change |
|------|--------|
| `rio-backend/src/config/distortion.rs` | **NEW** — `DistortionType` enum, `DistortionConfig` struct with effect, strength, center, animated, speed fields |
| `rio-backend/src/config/mod.rs` | Add `pub mod distortion`, import `DistortionConfig`, add `distortion` field to `Config` struct |
| `sugarloaf/src/components/distortion/mod.rs` | **NEW** — `DistortionParams` GPU struct, `DistortionBrush` struct with `new()`, `update_params()`, `render()` |
| `sugarloaf/src/components/distortion/distortion.wgsl` | **NEW** — Full-screen triangle vertex shader + fragment shader with barrel, perspective, wave, fisheye distortion functions |
| `sugarloaf/src/components/mod.rs` | Add `pub mod distortion` |
| `sugarloaf/src/sugarloaf.rs` | Add `distortion_brush` field to `Sugarloaf`, add `update_distortion()` method, insert distortion pass in `render()` between overlays and filters |
| `frontends/rioterm/src/renderer/mod.rs` | Convert `DistortionConfig` to `DistortionParams` and call `sugarloaf.update_distortion()` each frame |

## Dependencies

- No new crate dependencies
- Uses existing `bytemuck` (already in sugarloaf) for `Pod`/`Zeroable` derives
- Uses existing wgpu pipeline infrastructure (same pattern as `FiltersBrush`)
- Builds on the full-screen triangle pattern from `blit.wgsl`

## Testing

### Build Verification

```bash
cargo build -p rioterm
cargo clippy --all-targets --all-features -- -D warnings
cargo test --release
```

### Visual Verification

1. **Barrel distortion**: Set `effect = "barrel"`, `strength = 0.3` — edges
   of terminal content should curve outward (CRT bulge)
2. **Pincushion**: Set `strength = -0.3` — edges curve inward
3. **Perspective tilt**: Set `effect = "perspective"`, `strength = 0.5` —
   top of terminal appears farther away
4. **Wave animation**: Set `effect = "wave"`, `animated = true` — content
   gently ripples
5. **Fisheye**: Set `effect = "fisheye"`, `strength = 0.3` — center
   magnified, edges compressed
6. **Disabled**: Set `effect = "none"` — no visual change, no GPU overhead

### Performance Verification

1. With `effect = "none"` — zero overhead (brush is `None`)
2. With any effect — one texture copy + one full-screen triangle draw per
   frame (negligible on any GPU that already runs the terminal)
3. Animated effects request continuous redraws (same as indeterminate
   progress bar)

### Hot-Reload Verification

1. Change `effect` in config while terminal is running — effect switches
   immediately
2. Change `strength` — intensity updates without restart
3. Set `effect = "none"` — distortion brush is dropped, no residual cost

### Interaction with Filters

1. Enable both `distortion.effect = "barrel"` and
   `renderer.filters = ["newpixiecrt"]` — distortion applies first, then
   CRT scanlines/bloom render on top of the curved image

## Configuration Reference

### TOML

```toml
[distortion]
# Effect type: "none", "barrel", "perspective", "wave",
#              "fisheye"
effect = "barrel"

# Strength: 0.0 = off, positive = normal, negative = inverse
# Typical range: -1.0 to 1.0
strength = 0.3

# Center point (normalized 0.0–1.0)
center = [0.5, 0.5]

# Enable time-based animation (for wave effect)
animated = false

# Animation speed multiplier (1.0 = normal)
speed = 1.0
```

### Example Configurations

**Subtle CRT curvature:**
```toml
[distortion]
effect = "barrel"
strength = 0.15
```

**Dramatic fisheye:**
```toml
[distortion]
effect = "fisheye"
strength = 0.5
center = [0.5, 0.5]
```

**Gentle wave animation:**
```toml
[distortion]
effect = "wave"
strength = 0.2
animated = true
speed = 0.5
```

**CRT combo (distortion + RetroArch filter):**
```toml
[distortion]
effect = "barrel"
strength = 0.2

[renderer]
filters = ["newpixiecrt"]
```

## Future Enhancements

1. Per-split distortion — apply different effects to individual panes
2. Transition animations — smoothly interpolate between distortion states
3. Custom shader support — load user-provided WGSL distortion shaders
4. Vignette effect — darken edges (pairs well with barrel distortion)
5. Chromatic aberration — RGB channel offset at edges

## References

- `sugarloaf/src/components/filters/mod.rs` — FiltersBrush pattern (texture
  copy, full-screen pass, bind group layout)
- `sugarloaf/src/components/filters/shader/blit.wgsl` — Full-screen triangle
  vertex shader pattern
- `sugarloaf/src/sugarloaf.rs:509-516` — Existing post-processing integration
  point
- `sugarloaf/src/components/core/mod.rs:8-27` — Orthographic projection
  (unmodified by this CR)
