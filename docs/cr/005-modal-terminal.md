# CR-005: Leader Key Modal Dialog

**Status:** Implemented
**Date:** 2026-02-17
**Author:** wk

## Summary

Implement a leader key system that triggers a modal dialog popup, allowing users to select actions from a flat visual menu. Press leader key, see available actions, press a key to execute.

## Motivation

1. **Discoverability** - Users can see available actions without memorizing keybindings
2. **Simplicity** - Single entry point (leader key) to access all features
3. **Low conflict** - Only one key combination triggers the modal, minimal interference with programs

## User Flow

```
1. User presses leader key (default: Cmd+; on macOS, configurable via [leader] key)
2. Modal dialog appears at bottom-right with all actions
3. User presses key to select action
4. Action executes, modal closes
```

### Visual Example

```
+--------------------------------------------------+
| $ cargo build                                     |
|    Compiling rio v0.2.0                          |
|                                                  |
|                                                  |
|                                                  |
|                                                  |
|     +----------------------------------------+   |
|     |  Rio Commands                          |   |
|     +----------------------------------------+   |
|     |  n  New window        [  Prev tab      |   |
|     |  t  New tab           ]  Next tab      |   |
|     |  x  Close tab         y  Copy mode     |   |
|     |  v  Split down        /  Search        |   |
|     |  h  Split right       r  Reset         |   |
|     +----------------------------------------+   |
|     |  Press key or Esc to cancel            |   |
|     +----------------------------------------+   |
+--------------------------------------------------+
```

Note: Menu appears at bottom-right corner of the terminal.

## Architecture

### Data Structures

```rust
/// A single menu item in the leader menu
#[derive(Debug, Clone)]
pub struct LeaderMenuItem {
    pub key: char,                  // Key to press
    pub label: String,              // Display label
    pub action: LeaderItemAction,   // What to execute
}

/// Action type for a menu item
#[derive(Debug, Clone)]
pub enum LeaderItemAction {
    /// Execute a built-in Rio action
    Action(Action),
    /// Write text directly to PTY (as if user typed it)
    /// Supports variable substitution: ${SELECTION}, ${WORD}, ${LINE}, ${CWD}, ${FILE}
    Write(String),
}

/// Expand variables in write string
fn expand_variables(input: &str, ctx: &TerminalContext) -> String {
    input
        .replace("${SELECTION}", &ctx.selection().unwrap_or_default())
        .replace("${WORD}", &ctx.word_under_cursor().unwrap_or_default())
        .replace("${LINE}", &ctx.current_line().unwrap_or_default())
        .replace("${CWD}", &ctx.cwd().unwrap_or_default())
        .replace("${FILE}", &ctx.file_under_cursor().unwrap_or_default())
}

/// State of the leader menu
#[derive(Debug, Default)]
pub struct LeaderMenuState {
    pub active: bool,
    pub items: Vec<LeaderMenuItem>,
}
```

### Input Flow

```
KeyEvent
    |
    v
process_key_event()
    |
    +--> [Leader Menu Active?]
    |         |
    |        Yes --> handle_leader_input()
    |         |           |
    |         |           +--> Match key to menu item
    |         |           +--> Execute action
    |         |           +--> Close menu
    |         |
    |        No
    |         |
    +--> [Is Leader Key?]
              |
             Yes --> Open leader menu
              |
             No --> Normal PTY flow
```

### Default Menu Items

Default items are always available. Custom `[[leader.items]]` in config are **merged** with defaults - items with the same key override the default action.

| Key | Label | Action |
|-----|-------|--------|
| `n` | New window | `WindowCreateNew` |
| `t` | New tab | `TabCreateNew` |
| `x` | Close | `CloseCurrentSplitOrTab` |
| `[` | Prev tab | `SelectPrevTab` |
| `]` | Next tab | `SelectNextTab` |
| `s` | Split right | `SplitRight` |
| `v` | Split down | `SplitDown` |
| `h` | Pane left | `SelectSplitLeft` |
| `j` | Pane down | `SelectSplitDown` |
| `k` | Pane up | `SelectSplitUp` |
| `l` | Pane right | `SelectSplitRight` |
| `z` | Zoom pane | `ToggleZoom` |
| `y` | Copy mode | `ToggleViMode` |
| `/` | Search | `SearchForward` |
| `r` | Clear history | `ClearHistory` |

## Rendering

### Menu Panel

The menu is rendered as a bottom-right overlay panel:

```rust
/// Draw the leader menu overlay
pub fn draw_leader_menu(
    objects: &mut Vec<Object>,
    rich_text_id: usize,
    colors: &Colors,
    items: &[LeaderItem],
    dimensions: (f32, f32, f32),
) {
    let (width, height, scale) = dimensions;
    let scaled_width = width / scale;
    let scaled_height = height / scale;

    // Menu dimensions - auto-size based on items
    let item_height = 20.0;
    let padding = 16.0;
    let menu_width = 220.0_f32.min(scaled_width - 20.0);
    let menu_height = (items.len() as f32 * item_height + padding * 4.0)
        .min(scaled_height - 20.0);

    // Position at bottom-right with margin
    let margin = 10.0;
    let menu_x = scaled_width - menu_width - margin;
    let menu_y = scaled_height - menu_height - margin;

    // Draw background quad with rounded corners
    // Draw border quad
    // Draw rich text content
}
```

### Styling

```rust
// Default dark theme colors
const MENU_BG: Color = Color::rgba(30, 30, 46, 230);      // Semi-transparent
const MENU_BORDER: Color = Color::rgb(69, 71, 90);
const MENU_TEXT: Color = Color::rgb(205, 214, 244);
const MENU_KEY: Color = Color::rgb(137, 180, 250);        // Blue highlight
const MENU_HINT: Color = Color::rgb(127, 132, 156);       // Dimmed
```

## Configuration

```toml
[leader]
# Key to trigger leader menu (default: super+; which is Cmd+; on macOS)
# Supported modifiers: ctrl, alt/option, shift, super/cmd/command
key = "super+;"

# Custom menu items
[[leader.items]]
key = "n"
label = "New window"
action = "WindowCreateNew"

[[leader.items]]
key = "t"
label = "New tab"
action = "TabCreateNew"

[[leader.items]]
key = "x"
label = "Close tab"
action = "TabCloseCurrent"

[[leader.items]]
key = "v"
label = "Split down"
action = "SplitDown"

[[leader.items]]
key = "h"
label = "Split right"
action = "SplitRight"

[[leader.items]]
key = "y"
label = "Copy mode"
action = "ToggleViMode"

[[leader.items]]
key = "/"
label = "Search"
action = "SearchForward"

# Write text to terminal (as if user typed it)
[[leader.items]]
key = "g"
label = "Git status"
write = "git status\n"

[[leader.items]]
key = "e"
label = "Edit config"
write = "$EDITOR ~/.config/rio/config.toml\n"

[[leader.items]]
key = "l"
label = "List files"
write = "ls -la\n"
```

### Action Types

Each menu item supports one of three action types:

| Field | Description | Example |
|-------|-------------|---------|
| `action` | Execute built-in Rio action | `"TabCreateNew"` |
| `write` | Write text to PTY (as if typed) | `"git status\n"` |
| `exec` | Execute command in background | `"git push"` |

Note: Include `\n` at the end of `write` to execute the command.

#### Background Execution (`exec`)

The `exec` field runs a command in the background from the current working directory. While the command is running, an indeterminate (pulsing) progress bar is shown. When it completes:
- **Success** (exit code 0): Green progress bar
- **Error** (non-zero exit code): Red progress bar

```toml
# Git push in background
[[leader.items]]
key = "p"
label = "Git push"
exec = "git push"

# Run tests
[[leader.items]]
key = "T"
label = "Run tests"
exec = "cargo test"

# Format code
[[leader.items]]
key = "f"
label = "Format"
exec = "cargo fmt"
```

### Variables

Variables can be used in `write` and `exec` values using `${VAR}` syntax:

| Variable | Description |
|----------|-------------|
| `${SELECTION}` | Currently selected text |
| `${WORD}` | Word under cursor |
| `${LINE}` | Current line content |
| `${CWD}` | Current working directory |
| `${FILE}` | Detected file path under cursor |

#### Examples

```toml
# Open selected text in editor
[[leader.items]]
key = "o"
label = "Open selection"
write = "$EDITOR ${SELECTION}\n"

# Git blame current file
[[leader.items]]
key = "b"
label = "Git blame"
write = "git blame ${FILE}\n"

# CD to parent directory
[[leader.items]]
key = "u"
label = "Go up"
write = "cd ${CWD}/..\n"

# Search word in project
[[leader.items]]
key = "s"
label = "Search word"
write = "rg ${WORD}\n"
```

### Available Actions

All existing Rio actions can be used:

- `WindowCreateNew`, `Quit`
- `TabCreateNew`, `TabCloseCurrent`, `SelectNextTab`, `SelectPrevTab`, `SelectTab(n)`
- `SplitVertically`, `SplitHorizontally`, `CloseSplitOrTab`
- `ToggleViMode`, `SearchForward`, `SearchBackward`
- `ResetTerminal`, `ClearHistory`
- `Copy`, `Paste`
- `IncreaseFontSize`, `DecreaseFontSize`, `ResetFontSize`
- `Minimize`, `ToggleFullscreen`
- And more (see bindings documentation)

## Implementation Phases

### Phase 1: Core Menu System (Done)
1. Added `LeaderMenuState` to screen state
2. Implemented leader key detection (configurable, default: Cmd+;)
3. Added basic menu data structures in `rio-backend/src/config/leader.rs`
4. Handle menu input (key selection, Esc to close)

### Phase 2: Rendering (Done)
1. Created menu overlay renderer in `frontends/rioterm/src/renderer/leader.rs`
2. Draw menu panel at bottom-right with items
3. Uses theme bar color for background

### Phase 3: Actions (Done)
1. Connected menu items to existing Actions
2. Integrated with existing vi-mode and search
3. Support PTY write (shell commands via `write` field)

### Phase 4: Configuration (Done)
1. Added leader config section to Config
2. Support custom leader key via config (e.g., `key = "super+;"`, `key = "ctrl+space"`)
3. Support custom menu items with `action` or `write` fields
4. Variable expansion: `${SELECTION}`, `${WORD}`, `${LINE}`, `${CWD}`, `${FILE}`

## Files Modified

| File | Changes |
|------|---------|
| `frontends/rioterm/src/screen/mod.rs` | Added LeaderMenuState, input handling |
| `frontends/rioterm/src/screen/leader.rs` | New: menu state and logic |
| `frontends/rioterm/src/renderer/mod.rs` | Added LeaderMenu state, rendering integration |
| `frontends/rioterm/src/renderer/leader.rs` | New: menu overlay rendering |
| `frontends/rioterm/src/bindings/mod.rs` | Added ToggleLeaderMenu action, Ctrl+Space binding |
| `rio-backend/src/config/leader.rs` | New: leader config structs |
| `rio-backend/src/config/mod.rs` | Include leader config |

## References

- [which-key.nvim](https://github.com/folke/which-key.nvim)
- [Tmux prefix key](https://github.com/tmux/tmux/wiki/Getting-Started#prefix-key)
- [Helix space mode](https://docs.helix-editor.com/keymap.html#space-mode)
