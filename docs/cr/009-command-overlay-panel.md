# CR-009: Command Overlay Panel — Live PTY Floating Panel

**Status:** Implemented
**Date:** 2026-02-20
**Author:** wk

## Summary

Implement a command overlay panel — a floating, click-through panel that spawns a command in a real PTY and renders its live terminal output (full ANSI rendering) as an overlay on top of the terminal content. The overlay reuses Quick Terminal infrastructure (`Context` + `Crosswords` + `Machine` IO thread), is always click-through (keyboard input stays on the underlying pane), auto-dismisses when the command process exits, and is fully configurable via a `[command-overlay]` config section for position, size, appearance, borders, and shadows.

## Motivation

1. **Quick command glancing**: Users want to see system monitors (`top`, `htop`), git status (`git log --oneline`), disk usage (`df -h`), or other command output without leaving the current terminal session or disrupting the pane layout.

2. **Full terminal rendering**: Unlike simple text overlays, command output needs ANSI escape sequence rendering (colors, cursor positioning, line drawing) to properly display TUI programs like `htop` or `git log --graph`.

3. **Non-disruptive workflow**: The overlay is click-through — keyboard input always stays on the underlying pane. The user can keep typing while glancing at the floating panel.

4. **Reuses existing infrastructure**: The Quick Terminal (`ContextGrid::quick_terminal`) already solves the hard problem of spawning a PTY, running a terminal emulator, and rendering output. Command overlays reuse this exact infrastructure with a different shell config and floating layout.

5. **Configurable appearance**: Users should be able to customize the overlay's position, size, opacity, borders, corner radius, and shadow to match their workflow and theme.

## Architecture

### Data Flow

```
User presses keybinding              ContextManager                     ContextGrid
  overlay(top)                     toggle_command_overlay()          open_command_overlay()
       │                                    │                               │
       ▼                                    ▼                               ▼
Action::ToggleCommandOverlay ──► Parse command string ──► Create PTY Context ──► Store in
      ("top")                    into Shell { program,   with overridden Shell    command_overlays
                                 args }; set use_fork    and use_fork=false       Vec<CommandOverlayState>
                                 = false                                           │
                                                                                   ▼
                                                                          Compute pixel bounds
                                                                          from fractional config
                                                                          (x, y, width, height)
```

### Render Pipeline

```
Renderer::run()                           ContextGrid::extend_with_objects()
┌────────────────────────────┐           ┌──────────────────────────────────────┐
│                            │           │                                      │
│ For each command overlay:  │           │ For each visible overlay:            │
│   1. Lock terminal         │           │   1. Background Quad:               │
│   2. Take snapshot         │           │      - color from config or term bg │
│   3. Write lines to        │           │      - opacity from config          │
│      RichText (same as QT) │           │      - border_radius from config   │
│   4. Mark full damage      │           │      - border_color from config    │
│                            │           │      - shadow from config          │
│                            │           │   2. RichText Object:              │
│                            │           │      - terminal content from PTY   │
│                            │           │                                      │
└────────────────────────────┘           └──────────────────────────────────────┘
```

### State Management

```
ContextGrid<T>
  ├── inner: HashMap<usize, ContextGridItem<T>>     (normal panes)
  ├── quick_terminal: Option<QuickTerminalState<T>>  (QT overlay at bottom)
  ├── command_overlays: Vec<CommandOverlayState<T>>  (floating panels) ← NEW
  └── command_overlay_style: CommandOverlayStyle     (appearance config) ← NEW

CommandOverlayState<T>
  ├── item: ContextGridItem<T>     (PTY context + terminal + rich_text)
  ├── visible: bool                (toggle show/hide)
  ├── command: String              (identifier for toggle matching)
  └── bounds: CommandOverlayBounds (fractional x, y, width, height)
```

## Implementation Details

### 1. Action System — `overlay(command args)`

A new regex-parsed action string format was added to the Action enum:

```rust
// frontends/rioterm/src/bindings/mod.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // ...
    ToggleCommandOverlay(String),  // NEW
}

// Parsing: overlay(command args)
let re = regex::Regex::new(r"overlay\(([^()]+)\)").unwrap();
```

### 2. Config System — `[command-overlay]` Section

A new `CommandOverlayStyle` struct provides full appearance configuration:

```rust
// rio-backend/src/config/command_overlay.rs

pub struct CommandOverlayStyle {
    pub x: f32,                  // fractional position (default: 0.6)
    pub y: f32,                  // fractional position (default: 0.05)
    pub width: f32,              // fractional size (default: 0.38)
    pub height: f32,             // fractional size (default: 0.55)
    pub opacity: f32,            // 0.0–1.0 (default: 1.0)
    pub border_radius: f32,      // pixels (default: 6.0)
    pub border_width: f32,       // pixels (default: 1.0)
    pub border_color: ColorArray, // hex or [0,0,0,0] = use split color
    pub background_color: ColorArray, // hex or [0,0,0,0] = use term bg
    pub shadow_blur_radius: f32, // pixels (default: 0.0)
    pub shadow_color: ColorArray, // (default: #00000066)
    pub shadow_offset: [f32; 2], // pixels (default: [0.0, 2.0])
}
```

TOML configuration:

```toml
[command-overlay]
x = 0.6
y = 0.05
width = 0.38
height = 0.55
opacity = 0.95
border-radius = 6.0
border-width = 1.0
border-color = '#44475a'
background-color = '#282a36'
shadow-blur-radius = 8.0
shadow-color = '#00000066'
shadow-offset = [2.0, 4.0]
```

### 3. Leader Menu Integration

The `LeaderItem` struct gained an `overlay` field:

```rust
// rio-backend/src/config/leader.rs

pub struct LeaderItem {
    pub label: String,
    pub action: Option<String>,
    pub write: Option<String>,
    pub exec: Option<String>,
    pub overlay: Option<String>,  // NEW
}
```

### 4. PTY Context Creation

Command overlays reuse `ContextManager::create_context()` with overridden config:

```rust
// frontends/rioterm/src/context/mod.rs

pub fn toggle_command_overlay(&mut self, rich_text_id: usize, command: &str) {
    // Check for existing overlay with same command → toggle visibility
    let needs_creation = grid.toggle_command_overlay(command);
    if !needs_creation { return; }

    // Parse command string into Shell { program, args }
    let mut cloned_config = self.config.clone();
    cloned_config.shell = Shell { program: parts[0], args: parts[1..] };
    cloned_config.use_fork = false;  // Required for shell override

    // Compute overlay dimensions from config style
    let style = &self.config.command_overlay_style;
    let bounds = CommandOverlayBounds { x: style.x, y: style.y, ... };

    // Create PTY context and register overlay
    let new_context = ContextManager::create_context(..., &cloned_config)?;
    grid.open_command_overlay(new_context, command, bounds);
}
```

### 5. Process Exit Handling

When the command process exits, the overlay is automatically dismissed:

```rust
// frontends/rioterm/src/context/mod.rs

pub fn should_close_context_manager(&mut self, route_id: usize) -> bool {
    // Check command overlays first
    if grid.dismiss_command_overlay_by_route(route_id) {
        return false;  // Overlay dismissed, don't close the tab
    }
    // ... existing QT and split checks
}
```

### 6. Overlay Rendering

Command overlay terminal content is rendered in `Renderer::run()` after QT content, using the same snapshot-to-RichText pattern:

```rust
// frontends/rioterm/src/renderer/mod.rs

for overlay in &grid.command_overlays {
    if !overlay.visible { continue; }
    let terminal = overlay.item.val.terminal.lock();
    // ... take snapshot, write lines to RichText (same as QT)
}
```

Background quad rendering uses config values via `extend_with_objects()`:

```rust
// frontends/rioterm/src/context/grid.rs

let style = &self.command_overlay_style;
let bg = if style.has_custom_background() {
    let mut c = style.background_color;
    c[3] *= style.opacity;
    c
} else {
    let mut c = background_color;
    c[3] *= style.opacity;
    c
};
target.push(Object::Quad(Quad {
    position: pos,
    color: bg,
    size: [overlay_w, overlay_h],
    border_radius: [style.border_radius; 4],
    border_color: bc,
    border_width: style.border_width,
    shadow_color: style.shadow_color,
    shadow_offset: style.shadow_offset,
    shadow_blur_radius: style.shadow_blur_radius,
}));
```

### 7. Config Hot-Reload

The `[command-overlay]` config is updated on hot-reload:

```rust
// frontends/rioterm/src/screen/mod.rs — update_config()

self.context_manager.config.command_overlay_style = config.command_overlay;
for context_grid in self.context_manager.contexts_mut() {
    context_grid.command_overlay_style = config.command_overlay;
}
```

## Files Changed

| File | Change |
|------|--------|
| `rio-backend/src/config/command_overlay.rs` | **NEW** — `CommandOverlayStyle` config struct with position, size, opacity, border, shadow, background color fields |
| `rio-backend/src/config/mod.rs` | Add `pub mod command_overlay`, import `CommandOverlayStyle`, add `command_overlay` field to `Config` struct and `Default` impl |
| `rio-backend/src/config/leader.rs` | Add `overlay: Option<String>` field to `LeaderItem`; add `overlay: None` to all 15 default items |
| `frontends/rioterm/src/context/grid.rs` | Add `CommandOverlayState<T>`, `CommandOverlayBounds` structs; add `command_overlays` and `command_overlay_style` fields to `ContextGrid`; add `open_command_overlay()`, `toggle_command_overlay()`, `dismiss_command_overlay_by_route()`; extend `extend_with_objects()` for overlay rendering with config-driven appearance; update `objects()` |
| `frontends/rioterm/src/context/mod.rs` | Add `command_overlay_style` to `ContextManagerConfig`; add `toggle_command_overlay()` on `ContextManager`; extend `should_close_context_manager()` for overlay process exit |
| `frontends/rioterm/src/renderer/mod.rs` | Add command overlay terminal content rendering loop (after QT, before search/leader) |
| `frontends/rioterm/src/screen/mod.rs` | Add `Act::ToggleCommandOverlay` dispatch in both action locations; add `overlay` handling in `handle_leader_input()`; add config hot-reload for `command_overlay_style` |
| `frontends/rioterm/src/bindings/mod.rs` | Add `Action::ToggleCommandOverlay(String)` variant; add `overlay(command)` regex parsing |

## Dependencies

- CR-007 (overlay architecture design)
- CR-008 (batched overlay rendering fix)
- Existing Quick Terminal infrastructure (`Context`, `Crosswords`, `Machine` IO)
- Sugarloaf `Quad` primitive (already supports border, shadow, rounded corners)

## Testing

- **Build**: Clean compilation, no warnings
- **Unit tests**: All 600 tests pass
- **Visual testing**: Requires GUI runtime (manual verification)

## Configuration Reference

### Key Bindings

```toml
[bindings]
keys = [
  { key = "t", with = "super | shift", action = "overlay(top)" },
  { key = "h", with = "super | shift", action = "overlay(htop)" },
  { key = "g", with = "super | shift", action = "overlay(git log --oneline -20)" },
]
```

### Leader Menu

```toml
[[leader.items]]
label = "System Monitor"
overlay = "top"

[[leader.items]]
label = "Git Log"
overlay = "git log --oneline -20"
```

### Appearance

```toml
[command-overlay]
# Position (fractional 0.0–1.0 of window)
x = 0.6
y = 0.05

# Size (fractional 0.0–1.0 of window)
width = 0.38
height = 0.55

# Visual appearance
opacity = 0.95
border-radius = 6.0
border-width = 1.0
border-color = '#44475a'
background-color = '#282a36'

# Shadow (set shadow-blur-radius > 0 to enable)
shadow-blur-radius = 8.0
shadow-color = '#00000066'
shadow-offset = [2.0, 4.0]
```

## References

- CR-007: Multi-Layer Transparent Click-Through Overlay
- CR-008: Cursor Glow Overlay & Batched Overlay Rendering
- `sugarloaf/src/components/quad/mod.rs`: Quad GPU primitive (background, border, shadow)
