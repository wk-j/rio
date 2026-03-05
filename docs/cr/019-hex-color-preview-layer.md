# CR-019: Hex Color Preview Layer — Inline Color Overlay via Command Modal

**Status:** Proposed
**Date:** 2026-03-05
**Author:** wk

## Summary

Implement a color preview layer that scans visible terminal content for hex color codes (e.g., `#FF5733`, `#abc`, `0xFF5733`) and renders a colored quad directly on top of each detected hex code, covering the text entirely with the parsed color. The feature is triggered on-demand via the command modal dialog — not always-on — to avoid visual noise and rendering overhead during normal terminal usage. When activated, every hex color code on screen becomes a solid color block showing the actual color it represents. The underlying text and PTY state are unaffected.

## Motivation

1. **Developer productivity**: Developers frequently work with hex color codes in CSS, config files, themes, and terminal output. Seeing the actual color inline — without switching to a browser or color picker — provides instant visual feedback. Covering the hex text itself makes the mapping unambiguous: the text *becomes* the color.

2. **Non-disruptive by design**: Always-on color detection would add overhead to every frame and clutter the display. Triggering via the command modal makes it an intentional action: press a key, see colors, press again to dismiss. The hex text is still there underneath — toggle off and it's back.

3. **Leverages existing infrastructure**: The command overlay system (CR-009) already provides the action dispatch, toggle semantics, and overlay rendering pipeline. The color preview layer reuses the overlay quad rendering path (`extend_with_objects()` / `render_batch()`) without needing a PTY or terminal context.

4. **Terminal-native experience**: Unlike IDE extensions, this works on any terminal content — `cat` a CSS file, `grep` for colors in a config, pipe through `jq` — the preview layer detects and renders colors from whatever is on screen.

## User Flow

```
1. User is editing a CSS file or viewing terminal output containing hex colors:

   ┌─────────────────────────────────────────────┐
   │ $ cat style.css                             │
   │ body {                                      │
   │   background: #282a36;                      │
   │   color: #f8f8f2;                           │
   │   border: 1px solid #44475a;                │
   │ }                                           │
   │ .highlight { color: #FF79C6; }              │
   └─────────────────────────────────────────────┘

2. User triggers color preview via keybinding or leader menu:
   e.g., Super+Shift+C → Action::ToggleColorPreview

   ┌─────────────────────────────────────────────┐
   │ $ cat style.css                             │
   │ body {                                      │
   │   background: ███████;                      │
   │   color: ███████;                           │
   │   border: 1px solid ███████;                │
   │ }                                           │
   │ .highlight { color: ███████; }              │
   └─────────────────────────────────────────────┘
   (███████ = colored quad covering the hex code text,
    each block is the actual parsed color of the code it covers.
    e.g., #282a36 → dark gray block, #FF79C6 → pink block)

3. User presses the keybinding again → layer dismissed.
   Terminal content unchanged, no PTY interaction.
```

## Architecture

### Triggering

Unlike command overlays (CR-009) which spawn a PTY process, the color preview layer is a **pure visual overlay** — it reads terminal grid content, detects hex patterns, and emits colored quads. No PTY, no `Context`, no `Crosswords` instance needed.

A new `Action::ToggleColorPreview` variant is added. The toggle state is stored as a `bool` on `ContextGrid` (per-tab, so each tab can independently enable/disable the preview).

```
User presses keybinding              Screen                          ContextGrid
  or selects leader item          process_action()                color_preview_active
        │                               │                               │
        ▼                               ▼                               ▼
Action::ToggleColorPreview ──► grid.color_preview_active  ──►  Toggle bool + request
                                 = !current                     redraw (full damage)
```

### Detection Pipeline

```
Renderer::run()
├── For each visible pane context:
│   ├── Take terminal snapshot (existing step)
│   ├── Render terminal lines (existing step)
│   └── IF grid.color_preview_active:
│       ├── Scan snapshot.visible_rows for hex patterns
│       │   ├── Regex: #[0-9a-fA-F]{6,8} (6 or 8 digit with alpha)
│       │   ├── Regex: #[0-9a-fA-F]{3} (3-digit shorthand)
│       │   └── Regex: 0x[0-9a-fA-F]{6,8} (0x prefix variant)
│       ├── For each match:
│       │   ├── Parse hex string → ColorArray [r, g, b, a]
│       │   ├── Compute pixel position from (row, col_start) + pane offset
│       │   ├── Compute width from (col_end - col_start) * cell_w
│       │   └── Create covering Quad at same position as the hex text
│       └── Collect all covering Quads → color_preview_quads
└── sugarloaf.set_color_preview_overlay(color_preview_quads)
```

### Covering Quad Properties

Each detected color gets a quad rendered directly on top of the hex code text, covering it entirely with the parsed color:

| Property       | Value                                          |
|----------------|------------------------------------------------|
| Color          | Parsed hex value as `ColorArray`               |
| Position       | Same as first character of hex code: `pane_pos + col_start * cell_w` |
| Size           | `[(col_end - col_start) * cell_w, cell_h]` — spans exact width of hex text, full line height |
| Border radius  | `[2.0; 4]` (subtle rounding)                  |
| Border color   | Contrast border: white if color is dark, dark gray if color is light |
| Border width   | `1.0`                                          |
| Shadow         | None                                           |

### Luminance-Based Contrast Border

To ensure swatches are visible against any terminal background, the border color is computed from the swatch color's relative luminance:

```rust
fn contrast_border_color(color: &ColorArray) -> ColorArray {
    // Relative luminance (ITU-R BT.709)
    let lum = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
    if lum > 0.5 {
        [0.2, 0.2, 0.2, 0.8] // dark border for light colors
    } else {
        [0.9, 0.9, 0.9, 0.5] // light border for dark colors
    }
}
```

### State Management

```
ContextGrid<T>
  ├── inner: HashMap<usize, ContextGridItem<T>>
  ├── quick_terminal: Option<QuickTerminalState<T>>
  ├── command_overlays: Vec<CommandOverlayState<T>>
  ├── command_overlay_style: CommandOverlayStyle
  └── color_preview_active: bool                        ← NEW
```

```
SugarState
  ├── quads: Vec<Quad>
  ├── cursor_glow_overlay: Option<Quad>
  ├── vi_mode_overlay: Option<Quad>
  ├── visual_bell_overlay: Option<Quad>
  ├── progress_bar: Option<Quad>
  └── color_preview_quads: Vec<Quad>                    ← NEW
```

## Implementation Details

### 1. Action System — `ToggleColorPreview`

```rust
// frontends/rioterm/src/bindings/mod.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // ...
    ToggleColorPreview,  // NEW
}

// Parsing in action string matching:
"color-preview" => Action::ToggleColorPreview,
```

### 2. Grid State — `color_preview_active`

```rust
// frontends/rioterm/src/context/grid.rs

pub struct ContextGrid<T: EventListener> {
    // ... existing fields ...
    pub color_preview_active: bool,  // NEW — default false
}

impl<T: EventListener> ContextGrid<T> {
    pub fn toggle_color_preview(&mut self) {
        self.color_preview_active = !self.color_preview_active;
    }
}
```

### 3. Screen Action Dispatch

```rust
// frontends/rioterm/src/screen/mod.rs

Act::ToggleColorPreview => {
    let grid = self.context_manager.current_grid_mut();
    grid.toggle_color_preview();
    self.render();
}
```

### 4. Hex Detection — `detect_hex_colors()`

A new function in the renderer scans visible rows for hex color codes:

```rust
// frontends/rioterm/src/renderer/mod.rs

struct DetectedColor {
    row: usize,
    col_start: usize,   // first char of the hex code
    col_end: usize,      // one past last char
    color: ColorArray,
}

fn detect_hex_colors(
    visible_rows: &[Row<Square>],
    columns: usize,
) -> Vec<DetectedColor> {
    let mut results = Vec::new();

    for (row_idx, row) in visible_rows.iter().enumerate() {
        // Extract text content from row
        let mut text = String::with_capacity(columns);
        let mut col_map: Vec<usize> = Vec::with_capacity(columns);

        for col in 0..columns.min(row.len()) {
            let square = &row.inner[col];
            if square.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            col_map.push(col);
            text.push(square.c);
        }

        // Match hex patterns
        // Pattern: #RRGGBB, #RRGGBBAA, #RGB, 0xRRGGBB, 0xRRGGBBAA
        for m in HEX_COLOR_RE.find_iter(&text) {
            let hex_str = m.as_str();
            if let Some(color) = parse_hex_to_color(hex_str) {
                let byte_start = m.start();
                let byte_end = m.end();
                // Map byte offsets back to column positions
                let start_col = col_map[byte_start];
                let end_col = col_map[byte_end - 1] + 1;
                results.push(DetectedColor {
                    row: row_idx,
                    col_start: start_col,
                    col_end: end_col,
                    color,
                });
            }
        }
    }
    results
}
```

The regex is compiled once with `lazy_static!` or `std::sync::LazyLock`:

```rust
use std::sync::LazyLock;

static HEX_COLOR_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?:#[0-9a-fA-F]{8}|#[0-9a-fA-F]{6}|#[0-9a-fA-F]{3}(?![0-9a-fA-F])|0x[0-9a-fA-F]{6,8})"
    ).unwrap()
});
```

### 5. Hex Parsing — `parse_hex_to_color()`

Reuses the existing `hex_to_color_arr()` from `rio-backend::config::colors` where possible, with extensions for 3-digit and `0x` prefix forms:

```rust
fn parse_hex_to_color(s: &str) -> Option<ColorArray> {
    let hex = if let Some(stripped) = s.strip_prefix("0x") {
        stripped
    } else if let Some(stripped) = s.strip_prefix('#') {
        stripped
    } else {
        return None;
    };

    match hex.len() {
        3 => {
            // #RGB → #RRGGBB
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0])
        }
        _ => None,
    }
}
```

### 6. Covering Quad Generation

In `Renderer::run()`, after rendering terminal lines for each pane, if color preview is active, a colored quad is placed directly on top of each hex code — same position, same width as the text — so the hex code is fully covered by the color it represents:

```rust
// frontends/rioterm/src/renderer/mod.rs (inside Renderer::run)

if grid.color_preview_active {
    let mut color_quads: Vec<Quad> = Vec::new();

    for (key, grid_context) in grid.contexts() {
        let pane_pos = grid_context.position();
        let dim = &grid_context.val.dimension;
        let scale = dim.dimension.scale;
        let cell_w = dim.dimension.width / scale;
        let cell_h = (dim.dimension.height / scale)
            * dim.line_height;

        let detected = detect_hex_colors(
            &terminal_snapshot.visible_rows,
            *dim.columns,
        );

        for dc in &detected {
            // Position: exactly at the first character
            // of the hex code
            let quad_x = pane_pos[0]
                + (dc.col_start as f32) * cell_w;
            let quad_y = pane_pos[1]
                + (dc.row as f32) * cell_h;

            // Size: span the full width of the hex text
            // (#RRGGBB = 7 chars, #RGB = 4 chars, etc.)
            let char_count =
                (dc.col_end - dc.col_start) as f32;
            let quad_w = char_count * cell_w;
            let quad_h = cell_h;

            let border = contrast_border_color(&dc.color);

            color_quads.push(Quad {
                position: [quad_x, quad_y],
                size: [quad_w, quad_h],
                color: dc.color,
                border_radius: [2.0; 4],
                border_color: border,
                border_width: 1.0,
                shadow_color: [0.0; 4],
                shadow_offset: [0.0; 2],
                shadow_blur_radius: 0.0,
            });
        }
    }

    sugarloaf.set_color_preview_overlay(color_quads);
} else {
    sugarloaf.set_color_preview_overlay(Vec::new());
}
```

### 7. SugarState: New overlay field

```rust
// sugarloaf/src/sugarloaf/state.rs

pub struct SugarState {
    // ... existing fields ...
    pub color_preview_quads: Vec<Quad>,  // NEW
}

impl SugarState {
    pub fn set_color_preview_overlay(
        &mut self,
        quads: Vec<Quad>,
    ) {
        self.color_preview_quads = quads;
    }
}
```

### 8. Sugarloaf: Public API

```rust
// sugarloaf/src/sugarloaf.rs

impl Sugarloaf {
    pub fn set_color_preview_overlay(&mut self, quads: Vec<Quad>) {
        self.state.set_color_preview_overlay(quads);
    }
}
```

### 9. Batched Overlay Rendering

Color preview quads are appended to the existing overlay batch in `Sugarloaf::render()`:

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

    // NEW: append color preview swatches
    overlay_quads.extend_from_slice(&self.state.color_preview_quads);

    if !overlay_quads.is_empty() {
        let mut overlay_pass = encoder.begin_render_pass(/* LoadOp::Load */);
        self.quad_brush.render_batch(
            &mut self.ctx, &overlay_quads, &mut overlay_pass,
        );
    }
}
```

### 10. Leader Menu Integration

```rust
// rio-backend/src/config/leader.rs — default items

LeaderItem {
    label: String::from("Color Preview"),
    action: Some(String::from("color-preview")),
    write: None,
    exec: None,
    overlay: None,
}
```

## Files Changed

| File | Change |
|------|--------|
| `frontends/rioterm/src/bindings/mod.rs` | Add `Action::ToggleColorPreview` variant; add `"color-preview"` string parsing |
| `frontends/rioterm/src/context/grid.rs` | Add `color_preview_active: bool` field to `ContextGrid`; add `toggle_color_preview()` method; init to `false` |
| `frontends/rioterm/src/screen/mod.rs` | Add `Act::ToggleColorPreview` dispatch in both action handler locations; add leader menu support |
| `frontends/rioterm/src/renderer/mod.rs` | Add `detect_hex_colors()`, `parse_hex_to_color()`, `contrast_border_color()` functions; add color preview quad generation in `Renderer::run()` |
| `sugarloaf/src/sugarloaf/state.rs` | Add `color_preview_quads: Vec<Quad>` field to `SugarState`; add setter |
| `sugarloaf/src/sugarloaf.rs` | Add `set_color_preview_overlay()` public API; extend overlay batch in `render()` |
| `rio-backend/src/config/leader.rs` | Add "Color Preview" default leader item |

## Dependencies

- CR-007 (overlay architecture — color preview quads use the overlay render pass)
- CR-008 (batched overlay rendering — swatches are appended to the overlay batch)
- CR-009 (command overlay panel — action dispatch pattern reused)
- `regex` crate (already a dependency for `overlay(...)` parsing)

## Testing

### Manual Verification

1. **Basic hex detection**: `echo "#FF5733 #00FF00 #0000FF"` → three hex codes covered by their respective colors when toggled on
2. **Exact coverage**: The colored quad aligns precisely with the hex text — no gap before, no overshoot after. `#FF5733` (7 chars) gets a 7-cell-wide quad
3. **3-digit shorthand**: `echo "#F00 #0F0 #00F"` → 4-cell-wide red, green, blue blocks covering the text
4. **8-digit with alpha**: `echo "#FF573380"` → 9-cell-wide semi-transparent block (terminal background shows through)
5. **0x prefix**: `echo "0xFF5733"` → 8-cell-wide block covering `0xFF5733`
6. **File content**: `cat` a CSS/TOML/JSON file with hex colors → all hex codes on screen become color blocks
7. **Toggle on/off**: Press keybinding twice → color blocks appear covering hex text, then disappear revealing the text again
8. **Per-tab state**: Toggle in one tab, switch to another → second tab unaffected
9. **Split panes**: Color preview in one pane, quads positioned correctly within that pane's bounds
10. **Scrollback**: Scroll up/down while preview is active → color blocks update to match visible content
11. **Dark/light colors**: `echo "#000000 #FFFFFF"` → black block has light border, white block has dark border
12. **Multiple per line**: `echo "#FF0000 text #00FF00 text #0000FF"` → three separate blocks, surrounding text remains visible

### Automated Tests

- `test_parse_hex_6digit`: `parse_hex_to_color("#FF5733")` returns `[1.0, 0.341, 0.2, 1.0]`
- `test_parse_hex_3digit`: `parse_hex_to_color("#F00")` returns `[1.0, 0.0, 0.0, 1.0]`
- `test_parse_hex_8digit`: `parse_hex_to_color("#FF573380")` returns `[1.0, 0.341, 0.2, 0.502]`
- `test_parse_hex_0x_prefix`: `parse_hex_to_color("0xFF5733")` returns `[1.0, 0.341, 0.2, 1.0]`
- `test_parse_hex_invalid`: `parse_hex_to_color("#GGG")` returns `None`
- `test_detect_hex_colors_in_row`: Create mock `Row<Square>` with `#FF0000` content → detects 1 color at correct position
- `test_detect_no_false_positives`: Row with `#hello` or `#12` → no matches
- `test_contrast_border_dark`: `contrast_border_color([0.0, 0.0, 0.0, 1.0])` → light border
- `test_contrast_border_light`: `contrast_border_color([1.0, 1.0, 1.0, 1.0])` → dark border

### Regression

- Verify cursor glow, vi mode overlay, visual bell, and progress bar still render correctly when color preview is active (all coexist in the overlay batch)
- Verify no performance regression when color preview is off (zero overhead — no scanning, no quads)
- Verify toggle state survives config hot-reload

## Future Considerations

1. **Color format expansion**: Detect `rgb(255, 87, 51)`, `hsl(14, 100%, 60%)`, CSS named colors (`red`, `dodgerblue`)
2. **Tooltip on hover**: Show hex value, RGB breakdown, and color name when hovering over a color block
3. **Color picker integration**: Click a color block to open a color picker, write the new value back to the terminal (via PTY write)
4. **Text-over-color mode**: Instead of fully opaque covering, render the hex text on top of the color background with contrast foreground (readable text + color preview simultaneously)
5. **Performance optimization**: Cache detected colors per-line and only re-scan damaged lines (leverage the existing `TerminalDamage` system)

## Configuration Reference

### Key Bindings

```toml
[bindings]
keys = [
  { key = "c", with = "super | shift", action = "color-preview" },
]
```

### Leader Menu

```toml
[[leader.items]]
label = "Color Preview"
action = "color-preview"
```

## References

- CR-007: Multi-Layer Transparent Click-Through Overlay
- CR-008: Cursor Glow Overlay & Batched Overlay Rendering
- CR-009: Command Overlay Panel
- `rio-backend/src/config/colors/mod.rs`: `hex_to_color_arr()` — existing hex parsing
- `sugarloaf/src/components/quad/mod.rs`: Quad GPU primitive
