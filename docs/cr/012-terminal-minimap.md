# CR-012: Terminal Minimap

**Status:** Proposed
**Date:** 2026-02-24
**Author:** wk

## Summary

Add a terminal minimap — a read-only, click-through overlay panel on the right edge of the terminal that renders a compressed visual representation of the entire scrollback buffer using Braille Unicode characters. The minimap uses the `code-minimap` crate to convert terminal buffer lines into Braille dot patterns (each Braille character encodes a 2x4 pixel matrix, giving 8:1 vertical compression per character cell). A viewport indicator highlights the currently visible region. Clicking on the minimap scrolls the terminal to that buffer position. The feature is configurable via a `[minimap]` TOML section for width, opacity, colors, and toggle behavior.

## Motivation

1. **Buffer orientation**: Terminal scrollback can be thousands of lines deep. Users lose context about where they are in the buffer and what the overall structure looks like. A minimap provides a bird's-eye view of the entire buffer content, similar to code minimaps in editors like VS Code.

2. **Quick navigation**: Clicking on the minimap should jump the viewport to that position in the buffer, enabling faster navigation than repeated `PageUp`/`PageDown` or mouse wheel scrolling.

3. **Structure visualization**: The Braille-based rendering preserves the density and indentation structure of terminal output — command prompts, code blocks, log output, and blank regions are visually distinct even at minimap scale.

4. **Non-disruptive**: The minimap is click-through for keyboard input (reuses the existing overlay architecture from CR-007/CR-009). It overlays terminal content without affecting pane layout or input flow.

5. **Lightweight dependency**: The `code-minimap` crate is a focused, well-benchmarked Rust library (0.2ms for 79 lines, sub-millisecond for typical scrollback sizes). It has minimal dependencies (`itertools` only when used as a library).

## Architecture

### Data Flow

```
Terminal buffer (Grid<Square>)
         │
         ▼
Extract text lines from scrollback + visible rows
(iterate Grid[Line(-N)] for N in history..0, then visible rows)
         │
         ▼
Convert each Row<Square> to a String (character content only)
         │
         ▼
Feed lines as BufRead to code_minimap::write_to_string()
with hscale/vscale computed from minimap width and buffer height
         │
         ▼
Receive Braille Unicode string (compressed representation)
         │
         ▼
Push Braille text into minimap RichText via Content API
with foreground color from config (or terminal fg)
         │
         ▼
Add viewport indicator Quad (semi-transparent highlight
showing which portion of buffer is currently visible)
         │
         ▼
Add background Quad (minimap background panel)
         │
         ▼
Sugarloaf renders minimap RichText + Quads in main pass
```

### Click-to-Scroll Flow

```
Mouse click on minimap region
         │
         ▼
Detect click is within minimap bounds
(x >= minimap_x && x <= minimap_x + minimap_width)
         │
         ▼
Compute click_ratio = (click_y - minimap_y) / minimap_height
(0.0 = top of buffer, 1.0 = bottom)
         │
         ▼
Compute target_offset from click_ratio:
  total_lines = history_size + screen_lines
  target_line = click_ratio * total_lines
  target_offset = total_lines - target_line - screen_lines
         │
         ▼
terminal.scroll_display(Scroll::Delta(target_offset - current_offset))
```

### State Management

```
ContextGrid<T>
  ├── inner: HashMap<usize, ContextGridItem<T>>     (normal panes)
  ├── quick_terminal: Option<QuickTerminalState<T>>  (QT overlay)
  ├── command_overlays: Vec<CommandOverlayState<T>>  (floating panels)
  ├── minimap_state: Option<MinimapState>            ← NEW
  └── minimap_style: MinimapStyle                    ← NEW

MinimapState
  ├── rich_text_id: usize           (sugarloaf RichText instance)
  ├── content_cache: String         (last rendered Braille output)
  ├── cached_history_size: usize    (history size when cache was built)
  ├── cached_display_offset: usize  (display offset when cache was built)
  ├── visible: bool                 (toggle show/hide)
  └── pixel_bounds: MinimapBounds   (computed pixel x, y, width, height)
```

## Design

### Config Types (`rio-backend/src/config/minimap.rs`)

```rust
// rio-backend/src/config/minimap.rs

use serde::{Deserialize, Serialize};

pub type ColorArray = [f32; 4];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MinimapStyle {
    /// Whether the minimap is enabled (default: false).
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Minimap width as a fraction of the terminal width (default: 0.08).
    #[serde(default = "default_width")]
    pub width: f32,

    /// Background opacity (0.0–1.0, default: 0.85).
    #[serde(default = "default_opacity")]
    pub opacity: f32,

    /// Background color. [0,0,0,0] = use terminal background.
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_background_color"
    )]
    pub background_color: ColorArray,

    /// Foreground color for Braille characters. [0,0,0,0] = use
    /// terminal foreground.
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_foreground_color"
    )]
    pub foreground_color: ColorArray,

    /// Viewport indicator color (highlight showing visible region).
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_viewport_color"
    )]
    pub viewport_color: ColorArray,

    /// Horizontal scale factor for code-minimap (default: 0.5).
    /// Lower values = more horizontal compression.
    #[serde(default = "default_hscale")]
    pub hscale: f64,

    /// Vertical scale factor for code-minimap (default: 0.5).
    /// Lower values = more vertical compression.
    #[serde(default = "default_vscale")]
    pub vscale: f64,

    /// Border width in pixels (default: 1.0).
    #[serde(default = "default_border_width")]
    pub border_width: f32,

    /// Border color. [0,0,0,0] = use split color.
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_border_color"
    )]
    pub border_color: ColorArray,
}

fn default_enabled() -> bool { false }
fn default_width() -> f32 { 0.08 }
fn default_opacity() -> f32 { 0.85 }
fn default_background_color() -> ColorArray { [0.0, 0.0, 0.0, 0.0] }
fn default_foreground_color() -> ColorArray { [0.0, 0.0, 0.0, 0.0] }
fn default_viewport_color() -> ColorArray {
    [1.0, 1.0, 1.0, 0.15]
}
fn default_hscale() -> f64 { 0.5 }
fn default_vscale() -> f64 { 0.5 }
fn default_border_width() -> f32 { 1.0 }
fn default_border_color() -> ColorArray { [0.0, 0.0, 0.0, 0.0] }

impl Default for MinimapStyle {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            width: default_width(),
            opacity: default_opacity(),
            background_color: default_background_color(),
            foreground_color: default_foreground_color(),
            viewport_color: default_viewport_color(),
            hscale: default_hscale(),
            vscale: default_vscale(),
            border_width: default_border_width(),
            border_color: default_border_color(),
        }
    }
}

impl MinimapStyle {
    pub fn has_custom_background(&self) -> bool {
        self.background_color != [0.0, 0.0, 0.0, 0.0]
    }

    pub fn has_custom_foreground(&self) -> bool {
        self.foreground_color != [0.0, 0.0, 0.0, 0.0]
    }

    pub fn has_custom_border_color(&self) -> bool {
        self.border_color != [0.0, 0.0, 0.0, 0.0]
    }
}
```

### Example Config

Minimal — enable the minimap with defaults:

```toml
[minimap]
enabled = true
```

Full config:

```toml
[minimap]
enabled = true
width = 0.10
opacity = 0.9
hscale = 0.5
vscale = 0.5

# Colors (hex notation)
background-color = '#1e1e2e'
foreground-color = '#cdd6f4'
viewport-color = '#ffffff26'

# Border
border-width = 1.0
border-color = '#44475a'
```

### MinimapState (`frontends/rioterm/src/context/grid.rs`)

```rust
/// Pixel bounds of the minimap panel (computed from MinimapStyle
/// fractional width + window dimensions).
#[derive(Debug, Clone, Copy)]
pub struct MinimapBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Runtime state for the minimap overlay.
pub struct MinimapState {
    /// Sugarloaf RichText instance for rendering Braille content.
    pub rich_text_id: usize,
    /// Cached Braille string from code-minimap.
    pub content_cache: String,
    /// History size when cache was last built.
    pub cached_history_size: usize,
    /// Display offset when cache was last built.
    pub cached_display_offset: usize,
    /// Whether the minimap is currently visible.
    pub visible: bool,
    /// Computed pixel bounds for rendering and hit-testing.
    pub pixel_bounds: MinimapBounds,
}

impl MinimapState {
    /// Returns true if the cache needs to be rebuilt because buffer
    /// content or scroll position changed.
    pub fn needs_rebuild(
        &self,
        history_size: usize,
        display_offset: usize,
    ) -> bool {
        self.cached_history_size != history_size
            || self.cached_display_offset != display_offset
    }
}
```

### Braille Rendering via `code-minimap`

The `code-minimap` crate is used as a library (with `default-features = false` to exclude the CLI binary). Its core API:

```rust
pub fn write_to_string(
    reader: impl BufRead,
    hscale: f64,
    vscale: f64,
    padding: Option<usize>,
) -> io::Result<String>
```

Each Braille character encodes a 2-wide × 4-tall dot matrix. With `vscale = 0.5`, every 8 terminal lines map to approximately 1 Braille character row. For a 10,000-line scrollback, this produces ~1,250 Braille rows — well within a single RichText capacity.

### Action System

A new `Action::ToggleMinimap` variant is added:

```rust
// frontends/rioterm/src/bindings/mod.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // ...
    ToggleMinimap,
}

// Parsing (in the action string match block):
"toggleminimap" => Action::ToggleMinimap,
```

## Implementation Details

### 1. Config Module — `rio-backend/src/config/minimap.rs`

Create the new module with `MinimapStyle` as shown in the Design section. Uses the same `deserialize_to_arr` helper from the existing color deserialization infrastructure (used in `command_overlay.rs`).

Add to `rio-backend/src/config/mod.rs`:

```rust
pub mod minimap;

use crate::config::minimap::MinimapStyle;

// In the Config struct:
#[serde(default = "MinimapStyle::default", rename = "minimap")]
pub minimap: MinimapStyle,

// In Default for Config:
minimap: MinimapStyle::default(),
```

### 2. Minimap State — `frontends/rioterm/src/context/grid.rs`

Add `MinimapState`, `MinimapBounds` structs. Add fields to `ContextGrid`:

```rust
pub struct ContextGrid<T: EventListener> {
    // ... existing fields ...
    pub minimap_state: Option<MinimapState>,
    pub minimap_style: MinimapStyle,
}
```

Add methods on `ContextGrid`:

```rust
impl<T: EventListener> ContextGrid<T> {
    /// Initialize or toggle the minimap.
    pub fn toggle_minimap(
        &mut self,
        rich_text_id: Option<usize>,
    ) -> bool {
        if let Some(ref mut state) = self.minimap_state {
            state.visible = !state.visible;
            false // no new RichText needed
        } else if let Some(rt_id) = rich_text_id {
            self.minimap_state = Some(MinimapState {
                rich_text_id: rt_id,
                content_cache: String::new(),
                cached_history_size: 0,
                cached_display_offset: 0,
                visible: true,
                pixel_bounds: MinimapBounds {
                    x: 0.0, y: 0.0,
                    width: 0.0, height: 0.0,
                },
            });
            true // new RichText created
        } else {
            false
        }
    }

    /// Compute minimap pixel bounds from window dimensions.
    pub fn update_minimap_bounds(
        &mut self,
        window_width: f32,
        window_height: f32,
    ) {
        if let Some(ref mut state) = self.minimap_state {
            let style = &self.minimap_style;
            let minimap_w = window_width * style.width;
            let minimap_x = window_width - minimap_w;
            state.pixel_bounds = MinimapBounds {
                x: minimap_x,
                y: 0.0,
                width: minimap_w,
                height: window_height,
            };
        }
    }
}
```

Extend `extend_with_objects()` to add minimap background quad, viewport indicator quad, and RichText object:

```rust
// In extend_with_objects(), after command overlay objects:
if let Some(ref state) = self.minimap_state {
    if state.visible {
        let style = &self.minimap_style;
        let bounds = &state.pixel_bounds;

        // Background quad
        let bg = if style.has_custom_background() {
            let mut c = style.background_color;
            c[3] *= style.opacity;
            c
        } else {
            let mut c = background_color;
            c[3] *= style.opacity;
            c
        };
        let bc = if style.has_custom_border_color() {
            style.border_color
        } else {
            // Use split border color from theme
            background_color
        };
        target.push(Object::Quad(Quad {
            position: [bounds.x, bounds.y],
            color: bg,
            size: [bounds.width, bounds.height],
            border_radius: [0.0; 4],
            border_color: bc,
            border_width: style.border_width,
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_blur_radius: 0.0,
        }));

        // Viewport indicator quad (computed in renderer)
        // (viewport_quad is set by the renderer each frame)

        // RichText object
        target.push(Object::RichText(RichText {
            id: state.rich_text_id,
            position: [bounds.x, bounds.y],
            lines: None,
        }));
    }
}
```

### 3. Buffer-to-Text Extraction — `frontends/rioterm/src/renderer/mod.rs`

Add a helper method to extract terminal buffer content as a string:

```rust
impl Renderer {
    /// Extract all buffer lines (scrollback + visible) as a plain
    /// text string for minimap rendering.
    fn extract_buffer_text<T: EventListener>(
        &self,
        terminal: &Crosswords<T>,
    ) -> String {
        let history = terminal.history_size();
        let screen_lines = terminal.grid.screen_lines();
        let columns = terminal.grid.columns();
        let total = history + screen_lines;

        let mut text = String::with_capacity(total * (columns + 1));
        // Iterate from topmost scrollback line to bottom of screen
        for i in (0..total).rev() {
            let line_idx = Line(-(i as i32));
            let row = &terminal.grid[line_idx];
            for col_idx in 0..columns {
                let square = &row.inner[col_idx];
                text.push(square.c);
            }
            text.push('\n');
        }
        text
    }
}
```

### 4. Minimap Rendering in `Renderer::run()`

Add minimap rendering after command overlays and before search/leader overlays. The minimap does not use a PTY — it reads from the active pane's terminal buffer:

```rust
// In Renderer::run(), after command overlay rendering:

// Render minimap content
if let Some(ref mut minimap) = grid.minimap_state {
    if minimap.visible {
        // Lock the current pane's terminal (already locked above
        // for main pane rendering — relock for minimap)
        let terminal = contexts[grid.current].terminal.lock();
        let history_size = terminal.history_size();
        let display_offset = terminal.display_offset();
        let screen_lines = terminal.grid.screen_lines();
        let total_lines = history_size + screen_lines;

        // Rebuild cache if buffer changed
        if minimap.needs_rebuild(history_size, display_offset)
        {
            let text = self.extract_buffer_text(&terminal);
            let style = &grid.minimap_style;
            minimap.content_cache =
                code_minimap::write_to_string(
                    text.as_bytes(),
                    style.hscale,
                    style.vscale,
                    None,
                )
                .unwrap_or_default();
            minimap.cached_history_size = history_size;
            minimap.cached_display_offset = display_offset;
        }
        drop(terminal);

        // Write Braille content to minimap RichText
        let content = sugarloaf.content();
        content.sel(minimap.rich_text_id);
        content.clear();

        let fg_style = if grid.minimap_style.has_custom_foreground()
        {
            FragmentStyle {
                color: grid.minimap_style.foreground_color,
                ..FragmentStyle::default()
            }
        } else {
            FragmentStyle {
                color: foreground_color,
                ..FragmentStyle::default()
            }
        };

        for line in minimap.content_cache.lines() {
            content.add_text(line, fg_style);
            content.new_line();
        }
        content.build();

        // Compute viewport indicator quad
        if total_lines > 0 {
            let bounds = &minimap.pixel_bounds;
            let minimap_lines =
                minimap.content_cache.lines().count() as f32;
            if minimap_lines > 0.0 {
                let visible_ratio =
                    screen_lines as f32 / total_lines as f32;
                let offset_ratio = 1.0
                    - (display_offset as f32 + screen_lines as f32)
                        / total_lines as f32;
                let indicator_height =
                    bounds.height * visible_ratio;
                let indicator_y =
                    bounds.y + bounds.height * offset_ratio;

                // Push viewport indicator as an overlay quad
                sugarloaf.append_object(Object::Quad(Quad {
                    position: [bounds.x, indicator_y],
                    color: grid.minimap_style.viewport_color,
                    size: [bounds.width, indicator_height],
                    border_radius: [0.0; 4],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_blur_radius: 0.0,
                }));
            }
        }
    }
}
```

### 5. Click-to-Scroll — `frontends/rioterm/src/screen/mod.rs`

Add minimap click detection in the mouse click handler. The minimap occupies the right edge of the terminal, so check if the click falls within `minimap_state.pixel_bounds`:

```rust
// In Screen's mouse click handling (process_mouse_bindings or
// mouse_button_input), before normal pane click processing:

fn handle_minimap_click(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
    let grid = self.context_manager.current_grid();
    let minimap = match &grid.minimap_state {
        Some(m) if m.visible => m,
        _ => return false,
    };

    let bounds = &minimap.pixel_bounds;
    if mouse_x < bounds.x
        || mouse_x > bounds.x + bounds.width
        || mouse_y < bounds.y
        || mouse_y > bounds.y + bounds.height
    {
        return false;
    }

    // Compute target scroll position from click Y
    let click_ratio =
        (mouse_y - bounds.y) / bounds.height; // 0.0=top, 1.0=bottom

    let mut terminal =
        self.context_manager.current_mut().terminal.lock();
    let history_size = terminal.history_size();
    let screen_lines = terminal.grid.screen_lines();
    let total_lines = history_size + screen_lines;

    let target_line =
        (click_ratio * total_lines as f32) as usize;
    let target_offset = total_lines
        .saturating_sub(target_line + screen_lines);
    let current = terminal.display_offset() as i32;
    terminal.scroll_display(
        Scroll::Delta(target_offset as i32 - current),
    );

    true // consumed the click
}
```

### 6. Action Dispatch — `frontends/rioterm/src/screen/mod.rs`

Add `ToggleMinimap` action dispatch in `process_action()`:

```rust
Act::ToggleMinimap => {
    let grid = self.context_manager.current_grid_mut();
    if grid.minimap_state.is_none() {
        // First toggle: create RichText instance
        let rich_text_id =
            self.sugarloaf.create_rich_text();
        grid.toggle_minimap(Some(rich_text_id));
        // Compute initial bounds
        let (w, h) = self.sugarloaf.window_size();
        grid.update_minimap_bounds(w, h);
    } else {
        grid.toggle_minimap(None);
    }
}
```

### 7. Bindings — `frontends/rioterm/src/bindings/mod.rs`

Add the `ToggleMinimap` variant to `Action`:

```rust
/// Toggle the terminal minimap overlay.
ToggleMinimap,
```

Add parsing:

```rust
"toggleminimap" => Action::ToggleMinimap,
```

### 8. Config Hot-Reload — `frontends/rioterm/src/screen/mod.rs`

In `update_config()`:

```rust
self.context_manager.config.minimap = config.minimap;
for context_grid in self.context_manager.contexts_mut() {
    context_grid.minimap_style = config.minimap;
    // Recompute bounds in case width changed
    let (w, h) = /* window dimensions */;
    context_grid.update_minimap_bounds(w, h);
    // Invalidate cache to force rebuild with new scale settings
    if let Some(ref mut state) = context_grid.minimap_state {
        state.cached_history_size = 0;
    }
}
```

### 9. Window Resize Handling

When the window is resized, update minimap pixel bounds:

```rust
// In Screen's resize handler:
if let Some(ref mut _minimap) =
    self.context_manager.current_grid_mut().minimap_state
{
    let (w, h) = self.sugarloaf.window_size();
    self.context_manager
        .current_grid_mut()
        .update_minimap_bounds(w, h);
}
```

### 10. Caching Strategy

The minimap content is cached to avoid running `code-minimap` every frame. The cache is invalidated when:

- `history_size` changes (new lines added to scrollback)
- `display_offset` changes (user scrolled — viewport indicator moves)

For `display_offset` changes only, the Braille content cache remains valid — only the viewport indicator quad position needs updating. A more granular approach:

```rust
impl MinimapState {
    pub fn needs_content_rebuild(
        &self,
        history_size: usize,
    ) -> bool {
        self.cached_history_size != history_size
    }

    pub fn needs_viewport_update(
        &self,
        display_offset: usize,
    ) -> bool {
        self.cached_display_offset != display_offset
    }
}
```

This way, scrolling only updates the viewport indicator quad (a single Quad computation), while the expensive `code_minimap::write_to_string()` call only runs when new content enters the buffer.

## Files Changed

| File | Change |
|------|--------|
| `rio-backend/src/config/minimap.rs` | **NEW** — `MinimapStyle` config struct with width, opacity, colors, scale factors, border settings |
| `rio-backend/src/config/mod.rs` | Add `pub mod minimap`, import `MinimapStyle`, add `minimap` field to `Config` struct and `Default` impl |
| `frontends/rioterm/src/context/grid.rs` | Add `MinimapState`, `MinimapBounds` structs; add `minimap_state` and `minimap_style` fields to `ContextGrid`; add `toggle_minimap()`, `update_minimap_bounds()` methods; extend `extend_with_objects()` for minimap background quad and RichText object |
| `frontends/rioterm/src/context/mod.rs` | Add `minimap` to `ContextManagerConfig`; propagate `minimap_style` when creating grids |
| `frontends/rioterm/src/renderer/mod.rs` | Add `extract_buffer_text()` helper; add minimap rendering block in `run()` after command overlays (Braille content to RichText, viewport indicator quad) |
| `frontends/rioterm/src/screen/mod.rs` | Add `ToggleMinimap` action dispatch; add `handle_minimap_click()` for click-to-scroll; add config hot-reload for `minimap_style`; add resize handler for minimap bounds |
| `frontends/rioterm/src/bindings/mod.rs` | Add `Action::ToggleMinimap` variant and `"toggleminimap"` string parsing |
| `frontends/rioterm/Cargo.toml` | Add `code-minimap` dependency (library only, `default-features = false`) |
| `Cargo.toml` (workspace) | Add `code-minimap` to workspace dependencies |

## Dependencies

- **New crate**: `code-minimap` v0.6 (MIT/Apache-2.0) for Braille-based text minimap rendering. Added with `default-features = false` to exclude the CLI binary. Only transitive dependency is `itertools`.
- **Existing infrastructure**: Sugarloaf `Quad` primitive (background, viewport indicator), `RichText` system (Braille text rendering), `Content` API (text builder), `Scroll::Delta` (click-to-scroll viewport navigation).
- **CR dependencies**: None — the minimap uses the existing overlay rendering infrastructure established in CR-007/CR-008/CR-009 but does not depend on those CRs being implemented.

## Testing

### Unit Tests

1. **Config deserialization** (`rio-backend/src/config/minimap.rs`):
   - `test_default_minimap_style`: Default values match expected defaults (enabled=false, width=0.08, opacity=0.85, etc.).
   - `test_toml_kebab_case`: Verify `background-color`, `viewport-color`, `border-width` etc. deserialize correctly from TOML.
   - `test_has_custom_background`: Returns `false` for `[0,0,0,0]`, `true` for any other value.
   - `test_has_custom_foreground`: Same pattern.

2. **MinimapState cache invalidation** (`frontends/rioterm/src/context/grid.rs`):
   - `test_needs_content_rebuild`: Returns `true` when `history_size` differs, `false` when same.
   - `test_needs_viewport_update`: Returns `true` when `display_offset` differs.

3. **MinimapBounds computation** (`frontends/rioterm/src/context/grid.rs`):
   - `test_update_minimap_bounds`: With `width=0.08` and window `1000x800`, bounds should be `x=920, y=0, width=80, height=800`.
   - `test_toggle_minimap_creates_state`: First toggle with `Some(rich_text_id)` creates `MinimapState`.
   - `test_toggle_minimap_toggles_visibility`: Subsequent toggle flips `visible`.

4. **Buffer text extraction** (`frontends/rioterm/src/renderer/mod.rs`):
   - `test_extract_buffer_text`: Verify extracted text matches grid content for a known terminal state.

5. **Click-to-scroll math**:
   - `test_click_ratio_top`: Click at `y=0` with `history_size=1000, screen_lines=50` should scroll to top (offset near `history_size`).
   - `test_click_ratio_bottom`: Click at `y=height` should scroll to bottom (offset 0).
   - `test_click_ratio_middle`: Click at `y=height/2` should scroll to middle of buffer.

### Manual Verification

1. **Toggle minimap**: Bind `ToggleMinimap` to a key (e.g., `super+m`). Press to show/hide the minimap panel on the right edge.
2. **Visual structure**: Run `cat` on a source file with varied indentation. Verify the minimap shows the indentation structure as Braille dot density patterns.
3. **Large scrollback**: Run a command that produces thousands of lines (e.g., `seq 10000`). Verify the minimap compresses the entire buffer into the panel height.
4. **Viewport indicator**: Scroll up/down with mouse wheel. Verify the highlight rectangle moves correspondingly in the minimap.
5. **Click-to-scroll**: Click at different positions on the minimap. Verify the terminal viewport jumps to the corresponding buffer location.
6. **Config reload**: Change `minimap.opacity` or `minimap.width` while Rio is running. Verify the change takes effect after config reload.
7. **Resize**: Resize the terminal window. Verify the minimap repositions and resizes correctly.
8. **Multiple panes**: Open split panes. Verify the minimap shows content from the currently focused pane.
9. **Click-through**: With minimap visible, verify keyboard input goes to the underlying terminal pane (not consumed by the minimap).

### Performance Verification

- **Latency**: With 10,000 lines in scrollback, `code_minimap::write_to_string()` should complete in under 1ms. Verify no visible frame drops when toggling the minimap.
- **Caching**: After initial render, scrolling should not re-run `code-minimap` (only viewport indicator updates). Verify by adding `tracing::debug!()` around the cache rebuild path.
- **Memory**: The Braille string cache for 10,000 lines at `vscale=0.5` is approximately 1,250 lines × ~50 chars = ~60 KB. Minimal overhead.

## Configuration Reference

### Key Bindings

```toml
[bindings]
keys = [
  { key = "m", with = "super", action = "ToggleMinimap" },
]
```

### Appearance

```toml
[minimap]
# Enable/disable (default: false)
enabled = true

# Width as fraction of window (default: 0.08)
width = 0.10

# Background opacity (default: 0.85)
opacity = 0.9

# Scale factors for code-minimap compression
hscale = 0.5   # horizontal (default: 0.5)
vscale = 0.5   # vertical (default: 0.5)

# Colors (hex notation, #000000 = use terminal colors)
background-color = '#1e1e2e'
foreground-color = '#cdd6f4'
viewport-color = '#ffffff26'

# Border
border-width = 1.0
border-color = '#44475a'
```

## Future Work

1. **Scrollback size config**: CR-012 pairs well with making the scrollback buffer size configurable (currently hardcoded to 10,000 at `rio-backend/src/crosswords/mod.rs:451`). A `[scroll] scrollback-lines = 10000` config option would let users increase the buffer for longer minimap views.

2. **Color-aware minimap**: Instead of monochrome Braille characters, sample `Square.fg` colors from the buffer and apply them as `FragmentStyle` colors to each Braille line. This would give a color-coded minimap showing syntax highlighting or ANSI colors.

3. **Drag-to-scroll**: Instead of just click-to-scroll, support click-and-drag on the viewport indicator to continuously scroll the terminal while dragging.

4. **Per-pane minimap**: Show minimaps for all visible split panes simultaneously, each positioned at the right edge of its respective pane, rather than only the active pane.

5. **Animated viewport indicator**: Smooth-scroll the viewport indicator when the user scrolls, rather than jumping instantly.

6. **Minimap in command overlays**: Allow command overlay panels to have their own minimap, useful for long-running TUI programs.

## References

- `code-minimap` library: https://github.com/wfxr/code-minimap
- Braille Unicode block: U+2800–U+28FF (256 characters, 2×4 dot matrix)
- CR-009: Command Overlay Panel (overlay rendering pattern)
- CR-007: Multi-Layer Transparent Click-Through Overlay (overlay architecture)
- Rio terminal buffer: `rio-backend/src/crosswords/grid/mod.rs` (`Grid<Square>`, `Scroll` enum)
- Sugarloaf rendering: `sugarloaf/src/sugarloaf.rs` (`create_rich_text()`, `Content` API)
