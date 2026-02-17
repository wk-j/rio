# CR-004: Graphical Progress Bar (OSC 9;4)

**Status:** Implemented
**Date:** 2026-02-17
**Author:** wk

## Summary

Implement graphical progress bar support using the ConEmu `OSC 9;4` escape sequences, similar to Ghostty's implementation. The progress bar will be rendered as a native GUI element at the top of the terminal, supporting various states including numerical progress, indeterminate (pulsing), success, error, and warning states.

## Motivation

Progress bars provide visual feedback for long-running operations directly in the terminal UI. This feature is already supported by multiple terminals (Windows Terminal, ConEmu, Ghostty) and CLI tools (systemd, Zig compiler, Amp). Adding this to Rio will:

1. Improve UX for users running long operations
2. Maintain compatibility with existing tools that emit OSC 9;4 sequences
3. Pave the way for future enhancements (tab headers, dock icons, taskbar progress)

## Reference Implementation

Ghostty 1.2.0 introduced this feature (PR [#7975](https://github.com/ghostty-org/ghostty/issues/7975), [#8477](https://github.com/ghostty-org/ghostty/issues/8477)). From the release notes:

> Ghostty now recognizes the ConEmu `OSC 9;4` sequences and renders a GUI native progress bar. Progress bars can show success/error states, numerical progress towards completion, indeterminate progress (pulsing), and more.

## Protocol Specification

The ConEmu OSC 9;4 sequence format:

```
ESC ] 9 ; 4 ; <state> ; <progress> ST
```

Where:
- `ESC ]` is the OSC introducer (0x1B 0x5D)
- `ST` is the String Terminator (0x1B 0x5C or 0x07 BEL)
- `<state>` is a digit 0-5:
  - `0` = Hidden (remove progress bar)
  - `1` = Default/Normal (show progress) - blue
  - `2` = Error (red progress bar)
  - `3` = Indeterminate/Paused (pulsing animation) - blue
  - `4` = Warning (yellow progress bar)
  - `5` = Success (green progress bar) - Rio extension
- `<progress>` is 0-100 (percentage), optional for state 0 and 3

### Example Sequences

```bash
# Show 50% progress
printf '\e]9;4;1;50\e\\'

# Show error state at 75%
printf '\e]9;4;2;75\e\\'

# Show indeterminate/pulsing progress
printf '\e]9;4;3\e\\'

# Hide progress bar
printf '\e]9;4;0\e\\'

# Show warning at 100%
printf '\e]9;4;4;100\e\\'

# Show success (green) at 100% - Rio extension
printf '\e]9;4;5;100\e\\'
```

## Architecture

### Components

```
Terminal Input
    |
    v
VTE Parser (sugarloaf or rio-backend)
    |
    v
OSC Handler
    |
    +--> OSC 9;4 detected
    |
    v
ProgressState stored in Screen/Terminal state
    |
    v
Renderer reads ProgressState
    |
    v
GPU draws progress bar overlay
```

### Data Structures

```rust
/// Progress bar state from OSC 9;4
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ProgressState {
    #[default]
    Hidden,
    Normal { progress: u8 },      // 0-100, blue
    Error { progress: u8 },       // 0-100, red
    Indeterminate,                // pulsing animation, blue
    Warning { progress: u8 },     // 0-100, yellow
    Success { progress: u8 },     // 0-100, green (Rio extension)
}

impl ProgressState {
    pub fn is_visible(&self) -> bool {
        !matches!(self, ProgressState::Hidden)
    }
    
    pub fn progress_value(&self) -> Option<u8> {
        match self {
            ProgressState::Normal { progress } => Some(*progress),
            ProgressState::Error { progress } => Some(*progress),
            ProgressState::Warning { progress } => Some(*progress),
            _ => None,
        }
    }
}
```

### OSC Parsing

In the OSC handler (likely in `rio-backend/src/performer/handler.rs` or similar):

```rust
fn handle_osc(&mut self, params: &[&[u8]]) {
    match params {
        // OSC 9;4;<state>;<progress> - ConEmu progress bar
        [b"9", b"4", state, progress] => {
            let state = parse_digit(state);
            let progress = parse_progress(progress);
            self.set_progress_state(state, progress);
        }
        // OSC 9;4;<state> - state without progress (for hidden/indeterminate)
        [b"9", b"4", state] => {
            let state = parse_digit(state);
            self.set_progress_state(state, None);
        }
        // ... other OSC handlers
    }
}

fn set_progress_state(&mut self, state: Option<u8>, progress: Option<u8>) {
    let progress = progress.map(|p| p.min(100)).unwrap_or(0);
    self.progress_state = match state {
        Some(0) => ProgressState::Hidden,
        Some(1) => ProgressState::Normal { progress },
        Some(2) => ProgressState::Error { progress },
        Some(3) => ProgressState::Indeterminate,
        Some(4) => ProgressState::Warning { progress },
        Some(5) => ProgressState::Success { progress },
        _ => ProgressState::Hidden,
    };
}
```

### Rendering

The progress bar should be rendered as an overlay at the top of the terminal content area:

```rust
/// Progress bar rendering constants
const PROGRESS_BAR_HEIGHT: f32 = 3.0;  // pixels

/// Colors for different states
fn progress_bar_color(state: &ProgressState) -> [f32; 4] {
    match state {
        ProgressState::Normal { .. } => [0.2, 0.6, 1.0, 1.0],      // Blue
        ProgressState::Error { .. } => [1.0, 0.3, 0.3, 1.0],       // Red
        ProgressState::Warning { .. } => [1.0, 0.8, 0.2, 1.0],     // Yellow
        ProgressState::Success { .. } => [0.3, 0.8, 0.4, 1.0],     // Green
        ProgressState::Indeterminate => [0.2, 0.6, 1.0, 1.0],      // Blue (animated)
        ProgressState::Hidden => [0.0, 0.0, 0.0, 0.0],
    }
}
```

For the indeterminate state, implement a pulsing animation:

```rust
/// Calculate indeterminate progress bar position
/// Returns (start_x_ratio, width_ratio) for the animated segment
fn indeterminate_position(time: f32) -> (f32, f32) {
    let cycle = (time * 2.0) % 2.0;  // 2 second cycle
    let segment_width = 0.3;          // 30% of bar width
    
    if cycle < 1.0 {
        // Moving right
        let pos = cycle;
        (pos * (1.0 - segment_width), segment_width)
    } else {
        // Moving left
        let pos = 2.0 - cycle;
        (pos * (1.0 - segment_width), segment_width)
    }
}
```

### Integration Points

1. **Screen State** (`rio-backend/src/crosswords/mod.rs` or similar):
   - Add `progress_state: ProgressState` field to terminal state
   - Expose getter for renderer to query

2. **OSC Handler**:
   - Parse OSC 9;4 sequences
   - Update terminal's progress state

3. **Renderer** (`sugarloaf/` or `frontends/rioterm/`):
   - Query progress state before rendering frame
   - Draw progress bar overlay if visible
   - Handle indeterminate animation timing

4. **Event Loop**:
   - Request redraws during indeterminate animation

## Visual Design

```
Terminal window with progress bar:

+--------------------------------------------------+
|[===========================                    ] | <- 3px progress bar (60%)
|                                                  |
| $ cargo build                                    |
|    Compiling rio v0.2.0                          |
|    Compiling sugarloaf v0.1.0                    |
|                                                  |
+--------------------------------------------------+

Error state (red):
+--------------------------------------------------+
|[========================X                      ] | <- Red progress bar
|                                                  |

Indeterminate (pulsing):
+--------------------------------------------------+
|[      =====>                                   ] | <- Animated segment
|                                                  |
```

## Configuration

Add optional configuration to control progress bar behavior:

```toml
[renderer]
# Enable/disable progress bar rendering
progress-bar = true

# Progress bar height in pixels
progress-bar-height = 3

# Custom colors (optional, defaults to theme-aware colors)
# progress-bar-color = "#3B82F6"
# progress-bar-error-color = "#EF4444"
# progress-bar-warning-color = "#F59E0B"
```

## Implementation Phases (Completed)

### Phase 1: Basic Implementation (Done)
1. Added `ProgressState` enum to `rio-backend/src/ansi/mod.rs`
2. Added `progress_state` field to `Crosswords` struct
3. Implemented OSC 9;4 parsing in `performer/handler.rs`
4. Added `set_progress_state` method to `Handler` trait
5. Exposed progress state in `TerminalSnapshot`

### Phase 2: Rendering (Done)
1. Added `progress_bar` field to `SugarState`
2. Added `set_progress_bar` method to `Sugarloaf`
3. Rendered progress bar as overlay quad in render pass
4. Integrated progress bar in `renderer/mod.rs`

### Phase 3: Animation (Done)
1. Implemented indeterminate (pulsing) animation using time-based calculation
2. Animation uses 30% width segment that bounces left-to-right

### Future Enhancements
1. Add configuration options for progress bar appearance
2. Theme-aware colors (adapt to light/dark themes)
3. Extend to tab headers, dock badges
4. Continuous redraw timer for smoother indeterminate animation

## Shell Integration: Command Exit Status

You can use the progress bar to show command exit status by configuring your shell.

### Zsh (~/.zshrc)

```zsh
# Show command exit status in progress bar
precmd() {
  local exit_code=$?
  if [[ $exit_code -eq 0 ]]; then
    printf '\e]9;4;5;100\e\\'  # Success - green
  else
    printf '\e]9;4;2;100\e\\'  # Error - red
  fi
  # Auto-hide after 2 seconds
  (sleep 2 && printf '\e]9;4;0\e\\') &!
}
```

### Bash (~/.bashrc)

```bash
# Show command exit status in progress bar
PROMPT_COMMAND='
  exit_code=$?
  if [[ $exit_code -eq 0 ]]; then
    printf "\e]9;4;5;100\e\\"  # Success - green
  else
    printf "\e]9;4;2;100\e\\"  # Error - red
  fi
  (sleep 2 && printf "\e]9;4;0\e\\") &
'
```

### Fish (~/.config/fish/config.fish)

```fish
function __rio_postexec --on-event fish_postexec
    if test $status -eq 0
        printf '\e]9;4;5;100\e\\'  # Success - green
    else
        printf '\e]9;4;2;100\e\\'  # Error - red
    end
end
```

With auto-hide after 2 seconds:

```fish
function __rio_postexec --on-event fish_postexec
    if test $status -eq 0
        printf '\e]9;4;5;100\e\\'  # Success - green
    else
        printf '\e]9;4;2;100\e\\'  # Error - red
    end
    # Auto-hide after 2 seconds
    fish -c "sleep 2; printf '\e]9;4;0\e\\\\'" &
    disown
end
```

## Testing

### Manual Testing Script

```bash
#!/bin/bash
# test-progress.sh

echo "Testing progress bar..."

# Normal progress 0-100%
for i in $(seq 0 10 100); do
    printf '\e]9;4;1;%d\e\\' "$i"
    sleep 0.1
done

sleep 0.5

# Success state (green)
printf '\e]9;4;5;100\e\\'
sleep 1

# Error state (red)
printf '\e]9;4;2;75\e\\'
sleep 1

# Warning state (yellow)
printf '\e]9;4;4;100\e\\'
sleep 1

# Indeterminate (pulsing blue)
printf '\e]9;4;3\e\\'
sleep 3

# Hide
printf '\e]9;4;0\e\\'
echo "Done!"
```

### Integration Tests

- Verify OSC 9;4 parsing for all states
- Verify progress clamping (values > 100 should clamp to 100)
- Verify state transitions
- Verify progress bar hides on terminal reset

## Compatibility Notes

### Conflict with iTerm2 OSC 9 Notifications

As noted by Ghostty:

> The progress report `OSC 9;4` sequence collides with the iTerm2 notification sequence. Ghostty is the only emulator to support both sequences. To handle this, `OSC 9;4` always parses as a progress report, meaning you can't send any notifications starting with `;4` as notifications.

Rio should follow the same approach: prioritize `OSC 9;4` as progress bar since:
1. It has wider terminal support (ConEmu, Windows Terminal, Ghostty)
2. The `OSC 777` notification sequence is the more recommended alternative

## Dependencies

- No external crate dependencies required
- Uses existing VT parsing infrastructure
- Uses existing rendering pipeline

## Cargo Integration

Cargo checks for specific terminal programs to enable OSC 9;4 progress reporting. As of cargo 1.92+, it checks:
- `WT_SESSION` (Windows Terminal)
- `ConEmuANSI=ON` (ConEmu)
- `TERM_PROGRAM=WezTerm`
- `TERM_PROGRAM=ghostty`
- `TERM_PROGRAM=iTerm.app` with `TERM_FEATURES` containing "P"

Rio sets `TERM_PROGRAM=rio`, so cargo needs to be updated to recognize Rio. Until then, you can test by:

1. Setting `TERM_PROGRAM=ghostty` in your shell before running cargo
2. Or adding to `~/.cargo/config.toml`:
   ```toml
   [term]
   progress.term-integration = true
   ```

A PR should be submitted to rust-lang/cargo to add Rio to the supported terminals list.

## References

- [ConEmu ANSI Escape Codes Documentation](https://conemu.github.io/en/AnsiEscapeCodes.html#ConEmu_specific_OSC)
- [Ghostty 1.2.0 Release Notes](https://ghostty.org/docs/install/release-notes/1-2-0)
- [Windows Terminal Progress Bar Support](https://docs.microsoft.com/en-us/windows/terminal/)
- [Cargo shell.rs - Terminal Integration Detection](https://github.com/rust-lang/cargo/blob/master/src/cargo/core/shell.rs)
