# CR-006: Command Execute Status Indicator

**Status:** Implemented
**Date:** 2026-02-19
**Author:** wk

## Summary

Show a visual status indicator when a command finishes executing, displaying green for success (exit code 0) or red for failure (non-zero exit code). The indicator is rendered as a thin colored bar at the top of the terminal window, reusing the progress bar infrastructure from CR-004.

## Motivation

When working in a terminal, users frequently run commands and need immediate visual feedback about whether a command succeeded or failed. This is especially important for:

1. **Background commands** executed from the leader menu (`exec` field) where output is suppressed
2. **Long-running processes** where the user may have scrolled away or switched focus
3. **Shell integration** where per-command exit status can be surfaced in the terminal chrome

Traditional terminals rely on the user checking `$?` or reading error output. A glanceable status indicator at the top of the window provides instant, unambiguous feedback.

## Design

The command execute status indicator has two trigger paths:

### Path 1: Process Exit Detection

When a PTY child process exits, the raw exit status is captured and mapped to a visual state:

- **Exit code 0** -> Green bar (success)
- **Non-zero exit code** -> Red bar (error)
- **Unknown status** -> Hidden (no indicator)

This provides feedback when the shell session itself exits.

### Path 2: Background Command Execution (Leader Menu)

When a leader menu item uses the `exec` field, the command runs in a background thread:

1. An **indeterminate (pulsing blue)** progress bar appears immediately
2. The command runs in the background from the current working directory
3. On completion:
   - **Exit code 0** -> Green bar (success)
   - **Non-zero exit code** -> Red bar (error)

### Path 3: OSC 9;4 Shell Integration

Shells can emit OSC 9;4 escape sequences to explicitly set the indicator state after each command, enabling per-command status indicators within a live shell session (see [Shell Integration](#shell-integration) section).

## Architecture

```
                          Entry Points
                    /          |          \
                   /           |           \
    [PTY child exit]   [Leader exec]   [OSC 9;4 sequence]
          |                 |                  |
          v                 v                  v
    performer/mod.rs   screen/mod.rs     handler.rs
    ChildEvent::Exited execute_background  OSC parser
          |            _command()              |
          |                 |                  |
          v                 v                  v
    Set terminal       Send event       Crosswords
    progress_state     UpdateProgressBar set_progress_state()
          |                 |                  |
          v                 v                  v
          +--------+--------+--------+---------+
                   |
                   v
          terminal.progress_state
          (stored in Crosswords)
                   |
                   v
          TerminalSnapshot reads state
                   |
                   v
          renderer/mod.rs builds Quad
          (position, size, color)
                   |
                   v
          sugarloaf renders overlay
          (3px bar at top of window)
```

## Implementation Details

### ProgressState Enum

Defined in `rio-backend/src/ansi/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProgressState {
    #[default]
    Hidden,
    Normal { progress: u8 },      // Blue
    Error { progress: u8 },       // Red
    Indeterminate,                // Pulsing blue
    Warning { progress: u8 },     // Yellow
    Success { progress: u8 },     // Green (Rio extension)
}
```

The `Success` variant (state 5) is a Rio extension to the ConEmu OSC 9;4 protocol, which only defines states 0-4.

### Exit Code Mapping

In `rio-backend/src/performer/mod.rs`, when a child process exits:

```rust
// Unix: raw status from waitpid, extract with WEXITSTATUS
let exit_code = exit_status.map(|s| (s >> 8) & 0xff);
terminal.progress_state = match exit_code {
    Some(0) => ProgressState::Success { progress: 100 },
    Some(_) => ProgressState::Error { progress: 100 },
    None => ProgressState::Hidden,
};
```

### Background Command Event Flow

In `frontends/rioterm/src/screen/mod.rs`, the `execute_background_command()` method:

1. Resolves the current working directory via `foreground_process_path()`
2. Sets `ProgressState::Indeterminate` for immediate visual feedback
3. Marks `pending_update` with `TerminalDamage::Full` so the renderer processes the change
4. Spawns a `std::thread` that runs the command via `sh -c` (or `cmd /C` on Windows)
5. On completion, sends `RioEvent::UpdateProgressBar(exit_code)` to the event loop

In `frontends/rioterm/src/application.rs`, the event handler maps the exit code to a progress state and marks `pending_update` dirty before rendering:

```rust
RioEvent::UpdateProgressBar(exit_code) => {
    let progress_state = if exit_code == 0 {
        ProgressState::Success { progress: 100 }
    } else {
        ProgressState::Error { progress: 100 }
    };
    terminal.progress_state = progress_state;
    // Must mark pending_update dirty so the renderer processes the frame
    renderable_content.pending_update.set_ui_damage(TerminalDamage::Full);
}
```

**Important:** Both code paths must call `set_ui_damage(TerminalDamage::Full)` before `render()`. The renderer's `run()` method skips rendering when `pending_update.is_dirty()` is false, so without this the progress bar state change would be silently ignored. The OSC 9;4 path (Path 3) does not have this issue because `Crosswords::set_progress_state()` sends `RenderRoute`, which marks the pending update dirty via the application event handler.

### Rendering

In `frontends/rioterm/src/renderer/mod.rs`, the progress bar is constructed as a `Quad`:

| Property | Value |
|----------|-------|
| Position | `[0.0, 0.0]` (top-left corner) |
| Height | 3 pixels |
| Width | `window_width * (progress / 100)` |
| Color (Success) | `[0.3, 0.8, 0.4, 1.0]` (green) |
| Color (Error) | `[1.0, 0.3, 0.3, 1.0]` (red) |
| Color (Indeterminate) | `[0.2, 0.6, 1.0, 1.0]` (blue, animated) |

For the indeterminate state, a 30%-width segment bounces left to right on a 2-second cycle using `SystemTime`-based calculation.

For success, error, normal, and warning states, the bar animates with a **fill animation**: it grows from 0% to the target width over 300ms using a cubic ease-out curve (`1 - (1-t)^3`) for a fast-start, smooth-deceleration effect. The renderer tracks animation state via `progress_bar_anim_start` (an `Instant` recorded when the progress state transitions) and `progress_bar_last_state` (to detect transitions). During the animation, `pending_update` is marked dirty to ensure continuous redraws until the animation completes.

The quad is passed to `sugarloaf.set_progress_bar()` and rendered in a dedicated wgpu render pass labeled `"progress_bar"` as an overlay on top of all terminal content.

### Visual States

```
Success (exit code 0):
+--------------------------------------------------+
|[================================================]| <- Full green bar
| $ cargo build                                     |
|    Finished `dev` profile target(s) in 2.31s     |
+--------------------------------------------------+

Error (non-zero exit code):
+--------------------------------------------------+
|[================================================]| <- Full red bar
| $ cargo build                                     |
| error[E0308]: mismatched types                   |
+--------------------------------------------------+

Running (indeterminate):
+--------------------------------------------------+
|[      =====>                                    ] | <- Pulsing blue segment
| (command executing in background)                 |
+--------------------------------------------------+
```

## Shell Integration

Users can configure their shell to emit status indicators after each command:

### Zsh (~/.zshrc)

```zsh
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
    fish -c "sleep 2; printf '\e]9;4;0\e\\\\'" &
    disown
end
```

## Files Modified

| File | Changes |
|------|---------|
| `rio-backend/src/ansi/mod.rs` | `ProgressState` enum with `Success` variant, `from_osc()`, `is_visible()`, `progress_value()` |
| `rio-backend/src/crosswords/mod.rs` | `progress_state` field on `Crosswords`, `set_progress_state()` method |
| `rio-backend/src/performer/handler.rs` | OSC 9;4 parsing, `set_progress_state()` in `Handler` trait |
| `rio-backend/src/performer/mod.rs` | Child process exit status detection and mapping to `ProgressState` |
| `rio-backend/src/event/mod.rs` | `RioEvent::UpdateProgressBar(i32)` event variant |
| `frontends/rioterm/src/screen/mod.rs` | `execute_background_command()` with indeterminate state and background thread |
| `frontends/rioterm/src/application.rs` | `UpdateProgressBar` event handler mapping exit codes to states |
| `frontends/rioterm/src/context/renderable.rs` | `progress_state` field on `TerminalSnapshot` |
| `frontends/rioterm/src/renderer/mod.rs` | Progress bar quad construction with colors, fill animation (300ms ease-out), and animation state tracking (`progress_bar_anim_start`, `progress_bar_last_state`) |
| `sugarloaf/src/sugarloaf/state.rs` | `progress_bar: Option<Quad>` in `SugarState` |
| `sugarloaf/src/sugarloaf.rs` | Dedicated wgpu render pass for progress bar overlay |

## Testing

### Manual Test Script

```bash
#!/bin/bash
# Test command execute status indicator

# Test success
echo "Running successful command..."
true
# Should show green bar

# Test failure
echo "Running failing command..."
false
# Should show red bar
```

### Background Command Testing (via Leader Menu)

```toml
# In config.toml
[[leader.items]]
key = "p"
label = "Git push"
exec = "git push"

[[leader.items]]
key = "T"
label = "Run tests"
exec = "cargo test"
```

1. Press leader key, then `p` -> pulsing blue bar -> green or red on completion
2. Press leader key, then `T` -> pulsing blue bar -> green or red on completion

### OSC Sequence Testing

```bash
# test-status-indicator.sh
printf '\e]9;4;5;100\e\\'   # Green (success)
sleep 2
printf '\e]9;4;2;100\e\\'   # Red (error)
sleep 2
printf '\e]9;4;3\e\\'       # Pulsing (indeterminate)
sleep 3
printf '\e]9;4;0\e\\'       # Hidden
```

## Relationship to Other CRs

- **CR-004 (Progress Bar)**: This feature is built on top of the progress bar infrastructure. The status indicator reuses `ProgressState`, the rendering pipeline, and the OSC 9;4 protocol.
- **CR-005 (Leader Key Modal)**: The `exec` field in leader menu items triggers background commands that use the status indicator to show results.

## Future Enhancements

1. **Auto-hide timer**: Automatically hide the status indicator after a configurable delay
2. **Configuration**: Allow users to enable/disable, customize colors, and set bar height
3. **Notification integration**: Combine with system notifications for background command completion
4. **Tab/dock badge**: Extend status to tab headers and dock/taskbar badges
