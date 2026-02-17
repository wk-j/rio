# CR-005: Leader Key Modal Dialog

**Status:** Proposal
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
1. User presses leader key (e.g., Ctrl+Space)
2. Modal dialog appears with all actions
3. User presses key to select action
4. Action executes, modal closes
```

### Visual Example

```
+--------------------------------------------------+
| $ cargo build                                     |
|    Compiling rio v0.2.0                          |
|                                                  |
|   +----------------------------------------+     |
|   |  Rio Commands                          |     |
|   +----------------------------------------+     |
|   |  n  New window        c  Close window  |     |
|   |  t  New tab           x  Close tab     |     |
|   |  v  Split vertical    h  Split horiz   |     |
|   |  [  Previous tab      ]  Next tab      |     |
|   |  y  Copy mode         /  Search        |     |
|   |  =  Align windows     r  Reset term    |     |
|   +----------------------------------------+     |
|   |  Press key or Esc to cancel            |     |
|   +----------------------------------------+     |
|                                                  |
+--------------------------------------------------+
```

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
    Write(String),
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

If no `[[leader.items]]` are configured, use sensible defaults:

```rust
fn default_leader_items() -> Vec<LeaderMenuItem> {
    vec![
        item('n', "New window", LeaderItemAction::Action(Action::WindowCreateNew)),
        item('t', "New tab", LeaderItemAction::Action(Action::TabCreateNew)),
        item('x', "Close tab", LeaderItemAction::Action(Action::TabCloseCurrent)),
        item('v', "Split vertical", LeaderItemAction::Action(Action::SplitVertically)),
        item('h', "Split horizontal", LeaderItemAction::Action(Action::SplitHorizontally)),
        item('y', "Copy mode", LeaderItemAction::Action(Action::ToggleViMode)),
        item('/', "Search", LeaderItemAction::Action(Action::SearchForward)),
        item('r', "Reset terminal", LeaderItemAction::Action(Action::ResetTerminal)),
    ]
}
```
```

## Rendering

### Menu Panel

The menu should be rendered as a centered overlay panel:

```rust
pub struct LeaderMenuRenderer {
    background_color: Color,    // Semi-transparent dark
    border_color: Color,
    text_color: Color,
    key_color: Color,           // Highlighted key
}

impl LeaderMenuRenderer {
    pub fn render(&self, state: &LeaderMenuState, sugarloaf: &mut Sugarloaf) {
        // Calculate menu dimensions (2 columns)
        let columns = 2;
        let rows = (state.items.len() + 1) / columns;
        
        // Center in terminal
        let x = (terminal_width - width) / 2;
        let y = (terminal_height - height) / 2;
        
        // Draw background with border
        self.draw_panel(x, y, width, height);
        
        // Draw title
        self.draw_title("Rio Commands", x, y);
        
        // Draw menu items in 2 columns
        for (i, item) in state.items.iter().enumerate() {
            let col = i % columns;
            let row = i / columns;
            self.draw_item(item, x + col * col_width, y + row + 1);
        }
        
        // Draw footer hint
        self.draw_footer("Press key or Esc to cancel", x, y + height - 1);
    }
    
    fn draw_item(&self, item: &LeaderMenuItem, x: f32, y: f32) {
        // "  n  New window  "
        //  ^^^- key_color (highlighted)
        //      ^^^^^^^^^^^- text_color
    }
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
# Key to trigger leader menu
key = "ctrl+space"

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
label = "Split vertical"
action = "SplitVertically"

[[leader.items]]
key = "h"
label = "Split horizontal"
action = "SplitHorizontally"

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

Each menu item supports one of two action types:

| Field | Description | Example |
|-------|-------------|---------|
| `action` | Execute built-in Rio action | `"TabCreateNew"` |
| `write` | Write text to PTY (as if typed) | `"git status\n"` |

Note: Include `\n` at the end of `write` to execute the command.

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

### Phase 1: Core Menu System
1. Add `LeaderMenuState` to screen state
2. Implement leader key detection
3. Add basic menu data structures
4. Handle menu input (key selection, Esc to close)

### Phase 2: Rendering
1. Create menu overlay renderer in sugarloaf
2. Draw menu panel with items in 2-column layout
3. Style with semi-transparent background

### Phase 3: Actions
1. Connect menu items to existing Actions
2. Integrate with existing vi-mode and search
3. Support shell command execution

### Phase 4: Configuration
1. Add leader config section
2. Support custom leader key
3. Support custom menu items

## Files to Modify

| File | Changes |
|------|---------|
| `frontends/rioterm/src/screen/mod.rs` | Add LeaderMenuState, input handling |
| `frontends/rioterm/src/screen/leader.rs` | New: menu logic and default items |
| `sugarloaf/` | Add overlay panel rendering |
| `rio-backend/src/config/leader.rs` | New: leader config |
| `rio-backend/src/config/mod.rs` | Include leader config |

## References

- [which-key.nvim](https://github.com/folke/which-key.nvim)
- [Tmux prefix key](https://github.com/tmux/tmux/wiki/Getting-Started#prefix-key)
- [Helix space mode](https://docs.helix-editor.com/keymap.html#space-mode)
