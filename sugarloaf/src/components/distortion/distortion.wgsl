struct DistortionParams {
    distortion_type: u32,
    strength: f32,
    center: vec2<f32>,
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

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var uv = input.tex_coords;

    // 1 = barrel, 2 = perspective
    if params.distortion_type == 1u {
        uv = barrel_distort(
            uv, params.center, params.strength,
        );
    } else if params.distortion_type == 2u {
        uv = perspective_distort(
            uv, params.center, params.strength,
        );
    }

    // Out-of-bounds samples return black
    if uv.x < 0.0 || uv.x > 1.0
        || uv.y < 0.0 || uv.y > 1.0
    {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    return textureSample(src_texture, tex_sampler, uv);
}
