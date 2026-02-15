# CR-003: Preserve Transparent Background with Shader Filters

**Status:** Proposed
**Date:** 2026-02-15
**Author:** wk

## Summary

When shader filters (RetroArch `.slangp` presets) are active alongside window transparency (`opacity < 1.0`), the filter pipeline destroys the alpha channel, making the window fully opaque. Fix this by saving the alpha channel before the filter chain runs and restoring it afterward.

## Motivation

Rio supports both window transparency (`opacity = 0.8`, `blur = true`) and post-processing shader filters (`filters = ["newpixiecrt"]` or custom `.slangp` paths). These are both popular features, but they cannot be used together. When filters are enabled, the window loses all transparency because RetroArch shaders output `alpha = 1.0` on every pixel — they were designed for retro gaming, not transparent window compositing.

Users expect these features to compose naturally. A CRT shader effect over a semi-transparent, blurred terminal window is a desirable aesthetic.

## Architecture

### Rendering Pipeline (Current)

The render loop in `Sugarloaf::render()` (`sugarloaf/src/sugarloaf.rs`) executes in this order:

```
1. Clear framebuffer with background_color (alpha = opacity, e.g. 0.8)
2. Render bottom layer (background image, if any)
3. Render top layer graphics (sixel, iTerm2 inline images)
4. Render quads (cell backgrounds, cursors, selection, navigation)
5. Render rich text (glyphs)
6. Render overlays (vi mode, visual bell) — separate passes
7. Apply filter chain (FiltersBrush::render) — if filters configured
8. Present frame
```

Steps 1-6 correctly preserve the alpha channel. Step 7 destroys it.

### Filter Pipeline (Current)

`FiltersBrush::render()` in `sugarloaf/src/components/filters/mod.rs`:

```
1. Copy surface texture → source texture (alpha preserved)
2. Run librashader filter chain: source → [pass 0] → [pass 1] → ... → destination
3. Destination = surface texture (alpha destroyed by shader passes)
4. Present: compositor sees alpha = 1.0 everywhere → fully opaque window
```

### Surface Alpha Mode

The wgpu surface is configured with `CompositeAlphaMode::PostMultiplied` (preferred) or `PreMultiplied` (fallback) in `sugarloaf/src/context/mod.rs`. This is correct — the compositor respects the alpha channel. The problem is entirely that the filter chain writes `a=1.0`.

## Root Cause

Three issues combine to destroy transparency:

### Issue 1: RetroArch shaders output alpha = 1.0

`.slangp` shaders are HLSL/GLSL CRT/post-processing effects compiled via SPIR-V to WGSL. They operate on RGB channels and almost universally output fully opaque pixels. This is by design — retro game rendering has no concept of window transparency. Modifying third-party shaders is impractical.

### Issue 2: No blending in filter render pipelines

In `sugarloaf/src/components/filters/runtime/graphics_pipeline.rs`:

```rust
targets: &[Some(wgpu::ColorTargetState {
    format: framebuffer_format,
    blend: None,              // Replace semantics — overwrites destination entirely
    write_mask: wgpu::ColorWrites::ALL,
})],
```

The filter output fully replaces the render target content, including the alpha channel.

### Issue 3: Mipmap generation clears to opaque black

In `sugarloaf/src/components/filters/runtime/mipmap.rs`:

```rust
load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),  // BLACK.a = 1.0
```

Minor, but reinforces the lack of alpha awareness throughout the filter pipeline.

## Implementation

### Approach: Save and Restore Alpha

The most practical fix is to preserve the original alpha channel around the filter chain. This avoids modifying third-party shaders or the librashader integration. Changes are contained entirely within the filters module.

### 1. Alpha Restore Shader — `sugarloaf/src/components/filters/shader/alpha_restore.wgsl`

A small WGSL shader that composites the filter's RGB output with the original alpha:

```wgsl
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

// Full-screen triangle vertex shader (no vertex buffer needed)
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index & 2u) * 2 - 1);
    out.position = vec4<f32>(x, -y, 0.0, 1.0);
    out.tex_coords = vec2<f32>((x + 1.0) / 2.0, (1.0 - y) / 2.0);
    return out;
}

@group(0) @binding(0) var filtered_texture: texture_2d<f32>;
@group(0) @binding(1) var original_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let filtered = textureSample(filtered_texture, tex_sampler, in.tex_coords);
    let original = textureSample(original_texture, tex_sampler, in.tex_coords);
    return vec4<f32>(filtered.rgb, original.a);
}
```

### 2. Alpha Restore Pipeline — `sugarloaf/src/components/filters/mod.rs`

Add an alpha restore render pipeline and bind group to `FiltersBrush`:

```rust
pub struct FiltersBrush {
    filter_chain: FilterChainWgpu,
    // New fields for alpha restoration
    alpha_restore_pipeline: wgpu::RenderPipeline,
    alpha_restore_bind_group_layout: wgpu::BindGroupLayout,
    alpha_sampler: wgpu::Sampler,
}
```

Create the pipeline during `FiltersBrush::new()`:

```rust
let alpha_shader = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("Alpha Restore Shader"),
    source: wgpu::ShaderSource::Wgsl(
        include_str!("shader/alpha_restore.wgsl").into()
    ),
});

let alpha_restore_bind_group_layout =
    ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Alpha Restore BGL"),
        entries: &[
            // binding 0: filtered texture
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { /* 2d, float, non-filterable */ },
                count: None,
            },
            // binding 1: original texture (alpha source)
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { /* 2d, float, non-filterable */ },
                count: None,
            },
            // binding 2: sampler
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

let alpha_restore_pipeline =
    ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Alpha Restore Pipeline"),
        layout: Some(&ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&alpha_restore_bind_group_layout],
            ..Default::default()
        })),
        vertex: wgpu::VertexState {
            module: &alpha_shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: &alpha_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: ctx.format,
                blend: None,  // Replace — we're writing the final composited result
                write_mask: wgpu::ColorWrites::ALL,
            })],
            ..Default::default()
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        // no depth/stencil, no multisampling
        ..Default::default()
    });
```

### 3. Modified Render Flow — `FiltersBrush::render()`

```rust
pub fn render(
    &mut self,
    ctx: &Context,
    encoder: &mut wgpu::CommandEncoder,
    src_texture: &wgpu::Texture,
    dst_texture: &wgpu::Texture,
) {
    // 1. Copy source to a new texture (existing code, preserves alpha)
    let original_copy = Arc::new(ctx.device.create_texture(/* same as src */));
    encoder.copy_texture_to_texture(
        src_texture.as_image_copy(),
        original_copy.as_image_copy(),
        src_texture.size(),
    );

    // 2. Run the filter chain (existing code — alpha destroyed here)
    let filter_output = /* ... run filter chain, output to intermediate texture ... */;

    // 3. NEW: Alpha restore pass — combine filter RGB with original alpha
    let restore_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Alpha Restore BG"),
        layout: &self.alpha_restore_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &filter_output.create_view(&Default::default())
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &original_copy.create_view(&Default::default())
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&self.alpha_sampler),
            },
        ],
    });

    let dst_view = dst_texture.create_view(&Default::default());
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Alpha Restore Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &dst_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_pipeline(&self.alpha_restore_pipeline);
    pass.set_bind_group(0, &restore_bind_group, &[]);
    pass.draw(0..3, 0..1);  // Full-screen triangle
}
```

### 4. Fix Mipmap Clear Color — `sugarloaf/src/components/filters/runtime/mipmap.rs`

```rust
// Before:
load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),  // a = 1.0

// After:
load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),  // a = 0.0
```

## Data Flow (With Fix)

```
User config: opacity = 0.8, filters = ["ember-glow.slangp"]

1. Main render pass clears with alpha = 0.8            → surface has correct alpha
2. Content renders (quads, text, images)               → alpha preserved
3. FiltersBrush::render() called:
   a. Copy surface → original_copy                     → alpha = 0.8 saved
   b. Copy surface → filter_source                     → alpha = 0.8 (input to filters)
   c. Filter chain runs: filter_source → filter_output → alpha = 1.0 (destroyed by shaders)
   d. Alpha restore pass:
      output.rgb = filter_output.rgb                   → CRT/glow effects preserved
      output.a   = original_copy.a                     → alpha = 0.8 restored
   e. Write to surface                                 → surface has filtered RGB + original alpha
4. Present: compositor sees alpha = 0.8                → transparent window with shader effects
```

## Performance Impact

- One additional texture copy (already done; reusing the existing `original_copy`)
- One additional full-screen triangle render pass (alpha restore)
- One extra texture binding

The alpha restore pass is trivially cheap — a single full-screen triangle with two texture samples and no complex math. The filter chain itself (multiple passes with CRT math, blur kernels, scanline effects) dominates the cost. The overhead is negligible.

## Files Changed

| File | Change |
|---|---|
| `sugarloaf/src/components/filters/shader/alpha_restore.wgsl` | New file. Full-screen triangle shader that composites filtered RGB with original alpha. |
| `sugarloaf/src/components/filters/mod.rs` | Add alpha restore pipeline creation in `new()`. Modify `render()` to run alpha restore pass after filter chain. |
| `sugarloaf/src/components/filters/runtime/mipmap.rs` | Change mipmap clear color from `BLACK` (a=1.0) to `TRANSPARENT` (a=0.0). |

## Dependencies

- No new crate dependencies
- No configuration changes — works automatically when both `opacity < 1.0` and `filters` are set
- Backward compatible — when `opacity = 1.0`, the alpha restore pass writes `a = 1.0` (same as current behavior)
