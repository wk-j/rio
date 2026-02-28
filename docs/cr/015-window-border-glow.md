# CR-015: Window Border Glow Effect

**Status:** Proposed
**Date:** 2026-02-28
**Author:** wk

## Summary

Render a customizable glowing border effect around the inner edges of the terminal window, inspired by Opera GX's neon border aesthetic. The border is a GPU-rendered overlay consisting of four thin quads (one per edge) with configurable color, width, glow intensity, and optional animation. This brings a distinctive gaming/cyberpunk visual style to Rio.

## Motivation

1. **Visual identity**: Opera GX popularized glowing window borders as a defining visual feature. Users who want a gaming-terminal aesthetic currently have no way to add edge glow effects in Rio.

2. **Focus indicator**: A glowing border can serve as a strong visual cue for which terminal window is focused, especially in multi-window or tiling-WM setups where standard OS decorations are disabled.

3. **Leverages existing infrastructure**: The quad rendering system already supports solid color fills, border radius, and shadow blur. The overlay rendering pipeline (CR-007/CR-008) provides a compositing path that renders on top of all content. No shader modifications are required for the base implementation.

4. **Complements transparent/undecorated windows**: Users who set `window.decorations = "disabled"` or `"transparent"` lose the OS window border entirely. A rendered glow border restores visual boundary definition with a stylized look.

## Design

### Visual Reference

Opera GX draws a thin (~2-3px) colored border around the entire window frame with a soft outer glow that bleeds inward. The color is typically a vibrant neon (magenta, cyan, green) that matches the browser's accent theme. The effect is:

- Visible on all four edges (top, right, bottom, left)
- Respects rounded window corners when present
- Rendered inside the window surface (not as OS window decoration)
- Optionally animated (color cycling, pulse, or static)

### Approach: Multi-Quad Edge Composition

Render four edge quads positioned along the inner perimeter of the window surface. Each quad is a thin rectangle spanning the full edge length. To create the glow effect, use one of two techniques (configurable):

**Technique A — Shadow-based glow (preferred, no shader changes):**
Each edge quad has `border_width: 0`, a thin solid fill (the "core" line), and a `shadow_blur_radius` with `shadow_offset: [0, 0]` to create an omnidirectional soft glow around the core. The shadow SDF in the existing shader naturally produces a Gaussian falloff that reads as a glow.

**Technique B — Multi-layer stacking (fallback, proven pattern):**
Same approach as cursor glow (CR-008): stack 2-4 concentric quads per edge with increasing size and decreasing alpha. More GPU quads but guaranteed visual quality since it uses the same proven bloom technique.

### Rendering Layer

The border glow renders in the **overlay pass** (after text), ensuring it is always visible on top of terminal content. It is added as a new overlay category in `SugarState` alongside `cursor_glow_layers`, `vi_mode_overlay`, etc. The quads are included in the batched `render_batch()` call.

Rendering order within overlay pass:
1. Cursor glow layers (behind cursor)
2. **Window border glow** (new — behind vi mode tint but above cursor glow)
3. Vi mode overlay
4. Visual bell overlay
5. Progress bar

## Architecture

```
Config load                         Renderer::run()
┌─────────────┐                    ┌──────────────────────────┐
│ [window]    │                    │                          │
│ border-glow │──parse──┐         │ Read window dimensions   │
│   enabled   │         │         │ Read config border-glow  │
│   color     │         ▼         │                          │
│   width     │    BorderGlow     │ Compute 4 edge quads:    │
│   glow-*    │    config struct  │   top, right, bottom,    │
│   animate   │                   │   left                   │
└─────────────┘                   │                          │
                                  │ Apply shadow properties  │
                                  │ for glow effect          │
                                  │                          │
                                  │ set_window_border_glow() │
                                  └──────────┬───────────────┘
                                             │
                                             ▼
                                  Sugarloaf::render()
                                  ┌──────────────────────────┐
                                  │ Overlay pass:            │
                                  │  cursor_glow_layers      │
                                  │  window_border_glow ◄─── │
                                  │  vi_mode_overlay         │
                                  │  visual_bell_overlay     │
                                  │  progress_bar            │
                                  │                          │
                                  │ All batched in single    │
                                  │ render_batch() call      │
                                  └──────────────────────────┘
```

### Edge Quad Geometry

Given window dimensions `(W, H)` at scale factor `s`, border `width` in
logical pixels, and glow `spread` (shadow blur radius):

```
Top edge:    position=[0, 0],              size=[W, width]
Bottom edge: position=[0, H - width],      size=[W, width]
Left edge:   position=[0, width],          size=[width, H - 2*width]
Right edge:  position=[W - width, width],  size=[width, H - 2*width]
```

Left and right edges are inset vertically by `width` to avoid overlap at
corners. Alternatively, all four edges span the full length and the
alpha-blended overlap at corners produces a natural brightness boost
(corner accent effect).

### Corner Treatment

When `window.decorations` produces rounded OS corners (macOS default,
Windows 11), the border quads extend into corner regions. Two options:

1. **Simple (v1):** Let the quads overlap at corners. The glow's soft
   falloff makes the overlap look like a brighter corner highlight —
   visually acceptable and arguably desirable (Opera GX also has brighter
   corners).

2. **Rounded (v2, future):** Use a single large quad spanning the full
   window with `border_width` and `border_radius` matching the OS corner
   radius. This gives a perfect rounded-rectangle border. However, the
   current quad shader's border rendering uses a flat `border_color`
   without shadow/glow on the border band itself, so this would require
   shader modifications for glowing borders.

## Configuration

```toml
[window.border-glow]
enabled = false                # default off
color = "#8B5CF6"              # glow color (hex), default: purple
width = 2.0                    # core border width in logical pixels
glow-radius = 8.0              # shadow blur radius (glow spread)
glow-intensity = 0.6           # glow alpha multiplier (0.0–1.0)
animate = "none"               # "none", "pulse", "rainbow"
animate-speed = 1.0            # animation speed multiplier

# Planned future options:
# gradient = ["#8B5CF6", "#06B6D4"]  # two-stop gradient (requires shader work)
# only-when-focused = true            # hide glow on unfocused windows
```

### Config Struct

```rust
// rio-backend/src/config/window.rs (or new file window_border_glow.rs)

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BorderGlow {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_border_glow_color")]
    pub color: String,

    #[serde(default = "default_border_glow_width")]
    pub width: f32,

    #[serde(default = "default_border_glow_radius")]
    pub glow_radius: f32,

    #[serde(default = "default_border_glow_intensity")]
    pub glow_intensity: f32,

    #[serde(default)]
    pub animate: BorderGlowAnimate,

    #[serde(default = "default_border_glow_animate_speed")]
    pub animate_speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BorderGlowAnimate {
    #[default]
    None,
    Pulse,
    Rainbow,
}

fn default_border_glow_color() -> String {
    String::from("#8B5CF6")
}
fn default_border_glow_width() -> f32 {
    2.0
}
fn default_border_glow_radius() -> f32 {
    8.0
}
fn default_border_glow_intensity() -> f32 {
    0.6
}
fn default_border_glow_animate_speed() -> f32 {
    1.0
}

impl Default for BorderGlow {
    fn default() -> Self {
        Self {
            enabled: false,
            color: default_border_glow_color(),
            width: default_border_glow_width(),
            glow_radius: default_border_glow_radius(),
            glow_intensity: default_border_glow_intensity(),
            animate: BorderGlowAnimate::default(),
            animate_speed: default_border_glow_animate_speed(),
        }
    }
}
```

Add field to `Window`:

```rust
pub struct Window {
    // ... existing fields ...
    #[serde(default)]
    pub border_glow: BorderGlow,
}
```

## Implementation Details

### 1. SugarState: New overlay field

```rust
// sugarloaf/src/sugarloaf/state.rs
pub struct SugarState {
    // ... existing fields ...
    pub window_border_glow: Vec<Quad>,
}

impl SugarState {
    pub fn set_window_border_glow(&mut self, quads: Vec<Quad>) {
        self.window_border_glow = quads;
    }
}
```

Initialized to empty `Vec` in constructor. Not cleared by `reset()`.

### 2. Sugarloaf: Public API

```rust
// sugarloaf/src/sugarloaf.rs
impl Sugarloaf {
    pub fn set_window_border_glow(&mut self, quads: Vec<Quad>) {
        self.state.set_window_border_glow(quads);
    }
}
```

### 3. Renderer: Border glow computation

```rust
// frontends/rioterm/src/renderer/mod.rs (inside Renderer::run)
fn compute_border_glow(
    config: &BorderGlow,
    window_width: f32,
    window_height: f32,
    time: f32, // elapsed seconds, for animation
) -> Vec<Quad> {
    if !config.enabled {
        return Vec::new();
    }

    let color_rgb = parse_hex_color(&config.color);
    let alpha = match config.animate {
        BorderGlowAnimate::None => config.glow_intensity,
        BorderGlowAnimate::Pulse => {
            let t = (time * config.animate_speed * 2.0 * PI).sin();
            let base = config.glow_intensity;
            base * 0.5 + base * 0.5 * t // oscillate between 0 and full
        }
        BorderGlowAnimate::Rainbow => config.glow_intensity,
    };

    let color = match config.animate {
        BorderGlowAnimate::Rainbow => {
            hsl_to_rgb(
                (time * config.animate_speed * 60.0) % 360.0,
                0.8,
                0.6,
            )
        }
        _ => color_rgb,
    };

    let w = config.width;
    let blur = config.glow_radius;
    let shadow_color = [color[0], color[1], color[2], alpha];

    let make_edge = |pos: [f32; 2], size: [f32; 2]| -> Quad {
        Quad {
            position: pos,
            size,
            color: [color[0], color[1], color[2], alpha * 0.8],
            border_radius: [0.0; 4],
            border_color: [0.0; 4],
            border_width: 0.0,
            shadow_color,
            shadow_offset: [0.0, 0.0],
            shadow_blur_radius: blur,
        }
    };

    vec![
        // Top edge
        make_edge([0.0, 0.0], [window_width, w]),
        // Bottom edge
        make_edge([0.0, window_height - w], [window_width, w]),
        // Left edge (inset to avoid corner overlap)
        make_edge([0.0, w], [w, window_height - 2.0 * w]),
        // Right edge (inset to avoid corner overlap)
        make_edge(
            [window_width - w, w],
            [w, window_height - 2.0 * w],
        ),
    ]
}
```

### 4. Sugarloaf::render(): Include in overlay batch

```rust
// sugarloaf/src/sugarloaf.rs (inside render())
{
    let mut overlay_quads: Vec<Quad> = Vec::new();

    // Cursor glow (behind everything)
    overlay_quads.extend_from_slice(&self.state.cursor_glow_layers);

    // Window border glow
    overlay_quads.extend_from_slice(&self.state.window_border_glow);

    // Vi mode, visual bell, progress bar
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
        self.quad_brush.render_batch(
            &mut self.ctx,
            &overlay_quads,
            &mut overlay_pass,
        );
    }
}
```

### 5. Animation Loop

For `pulse` and `rainbow` animations, the renderer needs a monotonic
time source and must schedule continuous redraws:

```rust
// frontends/rioterm/src/renderer/mod.rs
pub struct Renderer {
    // ... existing fields ...
    pub border_glow_animating: bool,
    start_time: std::time::Instant,
}

// In Renderer::run():
let elapsed = self.start_time.elapsed().as_secs_f32();
let border_glow_quads = compute_border_glow(
    &config.window.border_glow,
    window_width,
    window_height,
    elapsed,
);
self.border_glow_animating =
    config.window.border_glow.enabled
    && config.window.border_glow.animate != BorderGlowAnimate::None;
sugarloaf.set_window_border_glow(border_glow_quads);
```

In `application.rs`, check `renderer.border_glow_animating` to schedule
redraws (same pattern as `trail_animating` in CR-008).

### 6. Config Hot-Reload

The border glow recomputes every frame from config values, so config
hot-reload works automatically. When `enabled` changes from `true` to
`false`, the next frame produces an empty `Vec<Quad>` and the glow
disappears. No special teardown needed.

## Files Changed

| File | Change |
|------|--------|
| `rio-backend/src/config/window.rs` | Add `BorderGlow` struct, `BorderGlowAnimate` enum, defaults; add `border_glow` field to `Window` |
| `sugarloaf/src/sugarloaf/state.rs` | Add `window_border_glow: Vec<Quad>` field, init, setter |
| `sugarloaf/src/sugarloaf.rs` | Add `set_window_border_glow()` API; include border glow quads in overlay batch |
| `frontends/rioterm/src/renderer/mod.rs` | Add `compute_border_glow()` function, `border_glow_animating` field, call in `run()` |
| `frontends/rioterm/src/application.rs` | Check `border_glow_animating` to schedule animation redraws |

## Dependencies

- CR-007 (overlay system architecture)
- CR-008 (batched overlay rendering — border glow uses the same `render_batch()` pipeline)

## Testing

### Unit Tests

- `test_border_glow_disabled`: Default config produces empty quad vec
- `test_border_glow_quad_positions`: Verify the four edge quads have correct positions and sizes for a given window dimension
- `test_border_glow_color_parse`: Verify hex color parsing for various formats (`#RGB`, `#RRGGBB`, `#RRGGBBAA`)
- `test_border_glow_pulse_animation`: Verify alpha oscillates over time
- `test_border_glow_rainbow_animation`: Verify color hue rotates over time
- `test_border_glow_config_deserialize`: Verify TOML round-trip for all fields

### Visual Tests

- **Static glow**: Enable with default purple color. All four edges should show a thin core line with a soft purple glow bleeding inward.
- **Pulse animation**: Set `animate = "pulse"`. The glow should smoothly brighten and dim.
- **Rainbow animation**: Set `animate = "rainbow"`. The glow color should cycle through the hue spectrum.
- **Resize**: Resize the window. The border glow should immediately adapt to new dimensions.
- **Splits**: The border glow should surround the entire window, not individual split panes.
- **Transparent window**: With `window.opacity < 1.0` or `window.decorations = "transparent"`, the glow should still render on the window surface edges.
- **Coexistence**: Border glow + cursor glow + progress bar should all render correctly (batched overlay).

## Future Work

- **Gradient border**: Two-color gradient along edges (requires adding gradient uniforms to the quad shader)
- **Per-edge color**: Different colors for each edge (top/right/bottom/left)
- **Focus-aware**: Show glow only when the window is focused; dim or hide on unfocused windows
- **Theme integration**: `color = "theme"` to derive glow color from the active color scheme's accent color
- **Corner radius matching**: Single rounded-rect border quad that matches OS window corner radius (requires shader modification for glowing borders)
- **Animated gradient flow**: Color flows along the border perimeter like a snake (requires fragment-position-aware color computation in the shader)

## References

- Opera GX window border: <https://www.opera.com/gx>
- CR-007: Multi-Layer Transparent Click-Through Overlay
- CR-008: Cursor Glow Overlay & Batched Overlay Rendering
- Quad shader SDF shadow: `sugarloaf/src/components/quad/shader/quad_f32_combined.wgsl`
