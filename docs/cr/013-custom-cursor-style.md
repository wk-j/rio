# CR-013: Fully Customizable Cursor Style

**Status:** Proposed
**Date:** 2026-02-24
**Author:** wk

## Summary

Extend Rio's cursor system to support fully customizable cursor styles beyond the four built-in shapes (block, underline, beam, hidden). Users will be able to define custom cursor geometry using a composable quad-based description in the TOML config, controlling dimensions, position offsets, border radius, border width, and opacity — enabling arbitrary cursor designs like rounded blocks, thick underlines, diamond shapes, L-shaped cursors, or multi-quad composite cursors.

## Motivation

1. **Personalization**: Terminal power users want their cursor to reflect their workflow and aesthetic preferences. The current four fixed shapes are limiting compared to what a GPU-accelerated renderer can achieve.

2. **Accessibility**: Users with visual impairments may need a cursor that is larger, thicker, or shaped differently from the standard options to track it effectively. A thick rounded underline or an oversized beam may be far more visible than the defaults.

3. **Differentiation**: No mainstream terminal emulator offers fully user-defined cursor geometry. This positions Rio as uniquely customizable among GPU-accelerated terminals.

4. **Foundation for themes**: Custom cursor styles can be shared as part of Rio theme configurations, enabling community-driven cursor designs.

## Architecture

### Design Approach

Rather than implementing an arbitrary vector path system (which would require a new GPU pipeline), this CR leverages the existing `Quad` primitive and `SugarCursor` rendering infrastructure. A custom cursor is defined as one or more quads relative to the cursor cell, each with configurable dimensions, position, color, border radius, and border width.

### Data Flow

```
TOML Config                    CursorConfig
┌────────────────────┐        ┌──────────────────────┐
│ [cursor]           │        │ shape: CursorShape    │
│ shape = 'custom'   │───────>│   Custom(Vec<        │
│                    │        │     CursorQuadDef>)   │
│ [[cursor.quads]]   │        │                       │
│ x = 0.0            │        └──────────┬─────────────┘
│ y = 0.8            │                   │
│ width = 1.0        │                   v
│ height = 0.2       │        Renderer: create_cursor_style()
│ border-radius = 2  │        ┌──────────────────────┐
│ ...                │        │ For each CursorQuadDef│
└────────────────────┘        │   → build SugarCursor │
                              │     ::Custom(quads)   │
                              └──────────┬─────────────┘
                                         │
                                         v
                              Compositor: render cursor
                              ┌──────────────────────┐
                              │ For each custom quad: │
                              │   resolve to pixel    │
                              │   coords relative to  │
                              │   cell, add_rect()    │
                              └───────────────────────┘
```

### Coordinate System

Custom cursor quads use a **cell-relative coordinate system**:

| Property | Range | Description |
|----------|-------|-------------|
| `x`      | 0.0–1.0 | Horizontal offset within cell (0.0 = left edge) |
| `y`      | 0.0–1.0 | Vertical offset within cell (0.0 = top edge) |
| `width`  | 0.0–2.0 | Width as fraction of cell width (>1.0 extends beyond cell) |
| `height` | 0.0–2.0 | Height as fraction of cell height (>1.0 extends beyond cell) |

This approach is resolution-independent — the cursor scales naturally with font size and DPI changes.

## Implementation Details

### 1. CursorQuadDef: Custom quad descriptor

```rust
// rio-backend/src/config/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CursorQuadDef {
    /// Horizontal offset within cell (0.0 = left edge, 1.0 = right edge)
    #[serde(default)]
    pub x: f32,
    /// Vertical offset within cell (0.0 = top edge, 1.0 = bottom edge)
    #[serde(default)]
    pub y: f32,
    /// Width as fraction of cell width
    #[serde(default = "default_quad_full")]
    pub width: f32,
    /// Height as fraction of cell height
    #[serde(default = "default_quad_full")]
    pub height: f32,
    /// Corner radius in pixels (0 = sharp corners)
    #[serde(default, rename = "border-radius")]
    pub border_radius: f32,
    /// Border width in pixels (0 = filled, >0 = outline only)
    #[serde(default, rename = "border-width")]
    pub border_width: f32,
    /// Opacity override (0.0–1.0). Default 1.0 = use cursor color as-is
    #[serde(default = "default_quad_full")]
    pub opacity: f32,
    /// Optional color override (hex string). If absent, uses cursor color.
    #[serde(default)]
    pub color: Option<String>,
}

#[inline]
fn default_quad_full() -> f32 {
    1.0
}
```

### 2. CursorShape: New Custom variant

```rust
// rio-backend/src/ansi/mod.rs
#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum CursorShape {
    #[default]
    #[serde(alias = "block")]
    Block,
    #[serde(alias = "underline")]
    Underline,
    #[serde(alias = "beam")]
    Beam,
    #[serde(alias = "hidden")]
    Hidden,
    /// User-defined cursor composed of one or more quads
    #[serde(alias = "custom")]
    Custom,
}
```

Note: `CursorShape` loses `Copy` since the custom quad definitions are stored in `CursorConfig`, not in the enum itself. The enum variant is just a discriminant; the actual quad data lives in `CursorConfig::quads`.

### 3. CursorConfig: Add quads field

```rust
// rio-backend/src/config/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CursorConfig {
    #[serde(default = "default_cursor")]
    pub shape: CursorShape,
    #[serde(default = "bool::default")]
    pub blinking: bool,
    #[serde(default = "default_cursor_interval", rename = "blinking-interval")]
    pub blinking_interval: u64,
    #[serde(default)]
    pub glow: CursorGlowConfig,
    /// Custom quad definitions. Only used when shape = 'custom'.
    #[serde(default)]
    pub quads: Vec<CursorQuadDef>,
}
```

### 4. SugarCursor: New Custom variant

```rust
// sugarloaf/src/sugarloaf/primitives.rs
#[derive(Debug, PartialEq, Clone)]
pub enum SugarCursor {
    Block([f32; 4]),
    HollowBlock([f32; 4]),
    Caret([f32; 4]),
    Underline([f32; 4]),
    /// Custom cursor: Vec of (relative_rect, color, border_radius, border_width)
    Custom(Vec<CustomCursorQuad>),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct CustomCursorQuad {
    /// Cell-relative position and size (x, y, width, height in 0.0–2.0 range)
    pub rel_rect: [f32; 4],
    /// RGBA color
    pub color: [f32; 4],
    /// Corner radius in pixels
    pub border_radius: f32,
    /// Border width in pixels (0 = filled)
    pub border_width: f32,
}
```

Note: `SugarCursor` loses `Copy` due to the `Vec` in `Custom`. Existing code that copies `SugarCursor` values will need `.clone()` — but cursor style is only created once per frame per pane, so the cost is negligible.

### 5. Renderer: Build custom SugarCursor from config

```rust
// frontends/rioterm/src/renderer/mod.rs (inside create_cursor_style)
CursorShape::Custom => {
    let quad_defs = &self.cursor_quads; // cached from config
    if quad_defs.is_empty() {
        // Fallback to block if no quads defined
        style.cursor = Some(SugarCursor::Block(cursor_color));
    } else {
        let custom_quads: Vec<CustomCursorQuad> = quad_defs
            .iter()
            .map(|def| {
                let color = def.color.as_ref()
                    .and_then(|hex| parse_hex_color(hex))
                    .map(|[r, g, b, a]| [r, g, b, a * def.opacity])
                    .unwrap_or_else(|| {
                        let mut c = cursor_color;
                        c[3] *= def.opacity;
                        c
                    });
                CustomCursorQuad {
                    rel_rect: [def.x, def.y, def.width, def.height],
                    color,
                    border_radius: def.border_radius,
                    border_width: def.border_width,
                }
            })
            .collect();
        style.cursor = Some(SugarCursor::Custom(custom_quads));
    }
}
```

### 6. Compositor: Render custom cursor quads

```rust
// sugarloaf/src/components/rich_text/compositor.rs
// Inside the cursor rendering match block:

crate::SugarCursor::Custom(ref quads) => {
    let font_height = style.ascent + style.descent;
    let cursor_top = style.baseline - style.ascent;

    for quad in quads {
        let qx = rect.x + quad.rel_rect[0] * rect.width;
        let qy = cursor_top + quad.rel_rect[1] * font_height;
        let qw = quad.rel_rect[2] * rect.width;
        let qh = quad.rel_rect[3] * font_height;

        if quad.border_width > 0.0 {
            // Outline: draw outer rect, then inner rect with bg
            let outer = Rect::new(qx, qy, qw, qh);
            self.batches.add_rect(&outer, depth, &quad.color);
            if let Some(bg_color) = style.background_color {
                let bw = quad.border_width;
                let inner = Rect::new(
                    qx + bw, qy + bw,
                    qw - bw * 2.0, qh - bw * 2.0,
                );
                self.batches.add_rect(&inner, depth, &bg_color);
            }
        } else {
            let cursor_rect = Rect::new(qx, qy, qw, qh);
            self.batches.add_rect(&cursor_rect, depth, &quad.color);
        }
        // Note: border_radius support requires Quad overlay path
        // (see Phase 2 below)
    }
}
```

### 7. Glow & Trail: Shape awareness for custom cursors

```rust
// frontends/rioterm/src/renderer/mod.rs (glow computation)
// For custom cursors, compute the bounding box of all quads:

CursorShape::Custom => {
    let quad_defs = &self.cursor_quads;
    if !quad_defs.is_empty() {
        let min_x = quad_defs.iter()
            .map(|q| q.x)
            .fold(f32::INFINITY, f32::min);
        let min_y = quad_defs.iter()
            .map(|q| q.y)
            .fold(f32::INFINITY, f32::min);
        let max_x = quad_defs.iter()
            .map(|q| q.x + q.width)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = quad_defs.iter()
            .map(|q| q.y + q.height)
            .fold(f32::NEG_INFINITY, f32::max);

        let glow_cx = cursor_x + (min_x + max_x) / 2.0 * cell_w;
        let glow_cy = cursor_y + (min_y + max_y) / 2.0 * cell_h;
        let glow_w = (max_x - min_x) * cell_w + glow_pad * 2.0;
        let glow_h = (max_y - min_y) * cell_h + glow_pad * 2.0;
        // Build glow quad centered on bounding box...
    }
}
```

### 8. DECSCUSR interaction

When a program sends DECSCUSR to change cursor shape (e.g., vim switching to beam in insert mode), it overrides the custom cursor temporarily. The custom style is restored when the program resets the cursor (DECSCUSR 0) or when the terminal is reset.

```rust
// rio-backend/src/crosswords/mod.rs (set_cursor_style)
fn set_cursor_style(&mut self, style: CursorShape, blinking: bool) {
    // DECSCUSR with parameter 0 restores default (which may be Custom)
    if matches!(style, CursorShape::Block) && !blinking {
        // Parameter 0 case — restore default
        self.cursor_shape = self.default_cursor_shape.clone();
    } else {
        self.cursor_shape = style;
    }
    // ...
}
```

### 9. Config validation

```rust
// rio-backend/src/config/mod.rs
impl CursorConfig {
    pub fn validate(&mut self) {
        if self.shape == CursorShape::Custom && self.quads.is_empty() {
            tracing::warn!(
                "cursor shape is 'custom' but no [[cursor.quads]] defined, \
                 falling back to 'block'"
            );
            self.shape = CursorShape::Block;
        }

        for quad in &mut self.quads {
            quad.x = quad.x.clamp(0.0, 2.0);
            quad.y = quad.y.clamp(0.0, 2.0);
            quad.width = quad.width.clamp(0.01, 2.0);
            quad.height = quad.height.clamp(0.01, 2.0);
            quad.opacity = quad.opacity.clamp(0.0, 1.0);
            quad.border_radius = quad.border_radius.max(0.0);
            quad.border_width = quad.border_width.max(0.0);
        }
    }
}
```

## Configuration

### Basic: Thick rounded underline

```toml
[cursor]
shape = 'custom'
blinking = true

[[cursor.quads]]
x = 0.0
y = 0.85
width = 1.0
height = 0.15
border-radius = 2
```

### Rounded block

```toml
[cursor]
shape = 'custom'

[[cursor.quads]]
x = 0.0
y = 0.0
width = 1.0
height = 1.0
border-radius = 4
```

### Hollow rounded block

```toml
[cursor]
shape = 'custom'

[[cursor.quads]]
x = 0.0
y = 0.0
width = 1.0
height = 1.0
border-radius = 4
border-width = 1.5
```

### Diamond / centered dot

```toml
[cursor]
shape = 'custom'

[[cursor.quads]]
x = 0.25
y = 0.25
width = 0.5
height = 0.5
border-radius = 100
```

### L-shaped cursor (multi-quad)

```toml
[cursor]
shape = 'custom'

# Vertical bar
[[cursor.quads]]
x = 0.0
y = 0.0
width = 0.15
height = 1.0

# Bottom horizontal bar
[[cursor.quads]]
x = 0.0
y = 0.85
width = 1.0
height = 0.15
```

### Two-tone cursor with color override

```toml
[cursor]
shape = 'custom'

# Main block (semi-transparent)
[[cursor.quads]]
x = 0.0
y = 0.0
width = 1.0
height = 1.0
opacity = 0.3

# Bright underline accent
[[cursor.quads]]
x = 0.0
y = 0.9
width = 1.0
height = 0.1
color = '#FF5555'
```

### Wide beam (thick caret)

```toml
[cursor]
shape = 'custom'

[[cursor.quads]]
x = 0.0
y = 0.0
width = 0.2
height = 1.0
border-radius = 1
```

## Files Changed

| File | Change |
|------|--------|
| `rio-backend/src/ansi/mod.rs` | Add `Custom` variant to `CursorShape`; remove `Copy` derive |
| `rio-backend/src/config/mod.rs` | Add `CursorQuadDef` struct; add `quads` field to `CursorConfig`; add `validate()` method |
| `rio-backend/src/config/defaults.rs` | Update default config template with custom cursor documentation |
| `rio-backend/src/crosswords/mod.rs` | Update `clone()` usage for `CursorShape` (no longer `Copy`) |
| `sugarloaf/src/sugarloaf/primitives.rs` | Add `CustomCursorQuad` struct; add `Custom` variant to `SugarCursor`; remove `Copy` derive |
| `sugarloaf/src/components/rich_text/compositor.rs` | Add `Custom` match arm in both cursor rendering blocks (drawable char + regular glyph paths) |
| `frontends/rioterm/src/renderer/mod.rs` | Add `cursor_quads` cached field; build `SugarCursor::Custom` in `create_cursor_style()`; update glow bounding box for custom shapes; update trail shape handling |
| `frontends/rioterm/src/screen/mod.rs` | Propagate `cursor_quads` on config reload |
| `frontends/rioterm/src/context/renderable.rs` | Handle `Custom` variant in `from_cursor_config()` |

## Implementation Phases

### Phase 1: Core custom cursor (filled rects)

1. Add `CursorQuadDef`, `Custom` variant, config parsing, validation
2. Add `CustomCursorQuad` and `SugarCursor::Custom` to sugarloaf
3. Implement compositor rendering (filled rects only — no border-radius in this phase since `add_rect` does not support rounded corners)
4. Wire up renderer and config reload
5. Handle `Copy` removal fallout across codebase

### Phase 2: Rounded corners via Quad overlay

1. For custom cursor quads with `border_radius > 0`, render them as `Quad` overlay primitives (which support `border_radius` natively) instead of compositor `add_rect` calls
2. Add a `custom_cursor_quads: Vec<Quad>` field to `SugarState`
3. Include custom cursor quads in the batched overlay render pass alongside glow, vi_mode, etc.
4. This gives full access to the Quad shader's rounded corners, border, and shadow features

### Phase 3: Config hot-reload and glow integration

1. Hot-reload custom cursor quads on config change
2. Compute glow bounding box from custom quad definitions
3. Trail ghost quads use the same custom shape
4. Add visual verification tests

## Dependencies

- CR-008 (Cursor Glow Overlay — glow/trail integration for custom shapes)
- Existing `Quad` GPU pipeline (for Phase 2 rounded corners)

## Testing

### Unit Tests

```rust
#[test]
fn test_custom_cursor_config_parsing() {
    let toml = r#"
        [cursor]
        shape = 'custom'
        [[cursor.quads]]
        x = 0.0
        y = 0.8
        width = 1.0
        height = 0.2
        border-radius = 2
    "#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.cursor.shape, CursorShape::Custom);
    assert_eq!(config.cursor.quads.len(), 1);
    assert_eq!(config.cursor.quads[0].y, 0.8);
    assert_eq!(config.cursor.quads[0].height, 0.2);
}

#[test]
fn test_custom_cursor_fallback_when_no_quads() {
    let toml = r#"
        [cursor]
        shape = 'custom'
    "#;
    let mut config: Config = toml::from_str(toml).unwrap();
    config.cursor.validate();
    assert_eq!(config.cursor.shape, CursorShape::Block);
}

#[test]
fn test_custom_cursor_quad_clamping() {
    let toml = r#"
        [cursor]
        shape = 'custom'
        [[cursor.quads]]
        x = -1.0
        y = 5.0
        width = 0.0
        height = 10.0
        opacity = 2.0
    "#;
    let mut config: Config = toml::from_str(toml).unwrap();
    config.cursor.validate();
    assert_eq!(config.cursor.quads[0].x, 0.0);
    assert_eq!(config.cursor.quads[0].y, 2.0);
    assert_eq!(config.cursor.quads[0].width, 0.01);
    assert_eq!(config.cursor.quads[0].height, 2.0);
    assert_eq!(config.cursor.quads[0].opacity, 1.0);
}
```

### Manual Visual Tests

1. **Thick underline**: Verify cursor appears as a fat bar at the bottom of the cell
2. **Rounded block**: Verify block cursor with rounded corners (Phase 2)
3. **Multi-quad**: Verify L-shaped cursor renders both quads correctly
4. **Blinking**: Verify custom cursor blinks at configured interval
5. **Glow**: Verify glow effect adapts to custom cursor bounding box
6. **Trail**: Verify trail ghosts match the custom cursor shape
7. **DECSCUSR**: Verify that vim/neovim can override cursor to beam, and custom shape restores on exit
8. **Hot-reload**: Verify changing `[[cursor.quads]]` in config updates cursor live
9. **Splits**: Verify custom cursor in active pane, hollow block in inactive panes

### Integration

- `cargo test -p rio-backend --release -- config::tests`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt -- --check`

## Future Work

- **Animated cursors**: Allow defining keyframe sequences for cursor quads (e.g., breathing/pulsing size changes)
- **Per-mode cursor styles**: Different custom cursor definitions for normal, insert, and visual modes (extending DECSCUSR)
- **Cursor theme sharing**: Standardized format for distributing cursor styles as part of Rio themes
- **Shadow support**: Expose the `Quad` shadow properties (shadow_color, shadow_offset, shadow_blur_radius) in `CursorQuadDef` for drop-shadow effects

## References

- CR-008: Cursor Glow Overlay & Batched Overlay Rendering
- Sugarloaf `Quad` struct: `sugarloaf/src/components/quad/mod.rs`
- Compositor cursor rendering: `sugarloaf/src/components/rich_text/compositor.rs:153-287`
- Renderer cursor style: `frontends/rioterm/src/renderer/mod.rs:700-800`
