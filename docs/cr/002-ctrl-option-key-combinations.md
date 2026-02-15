# CR-002: Fix Ctrl+Option Key Combinations on macOS

**Status:** Implemented
**Date:** 2026-02-15
**Author:** wk

## Summary

Fix Ctrl+Option+letter key combinations (e.g., Ctrl+Option+P, Ctrl+Option+N) on macOS to produce correct terminal escape sequences without any configuration. When Ctrl+Option are pressed together, Option is now always treated as Alt — regardless of the `option-as-alt` setting — because Ctrl+Option+letter has no useful macOS compose meaning.

## Motivation

Terminal applications like Emacs, tmux, and shell readline use Ctrl+Alt (Ctrl+Option on macOS) combinations extensively. For example:
- `Ctrl+Alt+P` / `Ctrl+Alt+N` — history navigation in some shells
- `Ctrl+Alt+F` / `Ctrl+Alt+B` — word-forward / word-backward in Emacs

Ghostty and other terminals handle these correctly out of the box. Rio was not producing any output or producing incorrect bytes for these combinations. Users had to work around this by adding manual byte bindings in their config:

```toml
{ key = "u", with = "control | alt", bytes = [27, 21] },
{ key = "p", with = "control | alt", bytes = [27, 16] },
```

This workaround was required for every Ctrl+Alt+letter combination individually, which is impractical.

## Architecture

The keyboard input pipeline on macOS flows through these layers:

```
NSEvent (macOS)
  -> replace_event()          [rio-window: view.rs]   — rewrites event for option-as-alt
  -> create_key_event()       [rio-window: event.rs]  — converts to KeyEvent
  -> process_key_event()      [rioterm: screen/mod.rs] — generates bytes for PTY
```

The non-kitty keyboard path in `process_key_event` sends `text_with_all_modifiers` (from `NSEvent.characters()`) directly to the PTY. It does not compute control characters itself — it relies entirely on what `NSEvent.characters()` returns. If `alt_send_esc()` returns true, an ESC (`\x1b`) byte is prepended.

## Root Cause

Three problems combined to break Ctrl+Option+letter:

### Problem 1: replace_event skipped rewriting when Ctrl was held

The `replace_event` function (`rio-window/src/platform_impl/macos/view.rs`) had a `!ev_mods.control_key()` guard:

```rust
let ignore_alt_characters = match option_as_alt {
    ...
} && !ev_mods.control_key()    // <-- BLOCKED CTRL+OPTION
    && !ev_mods.super_key();
```

When Ctrl was held alongside Option, the event was never rewritten, so macOS applied Ctrl to the Option-composed character (e.g., Ctrl+"π" instead of Ctrl+"p"), producing garbage.

### Problem 2: charactersIgnoringModifiers strips Ctrl too

Simply removing the `!ev_mods.control_key()` guard was insufficient. `NSEvent.charactersIgnoringModifiers()` strips ALL modifier effects including Ctrl. The rewritten event's `characters` field ended up as `"p"` instead of `"\x10"` (Ctrl-P). The PTY would receive `\x1b p` (ESC + literal p) instead of `\x1b\x10` (ESC + Ctrl-P).

### Problem 3: option-as-alt config gatekept the entire fix

Both `replace_event` and `alt_send_esc()` were conditional on `option-as-alt` being configured. With the default `option-as-alt = "none"`, neither function activated for Ctrl+Option combinations, so no fix could take effect without the user changing their config.

## Implementation

### 1. Ctrl Character Transform — `rio-window/src/platform_impl/macos/view.rs`

New helper function that maps a base ASCII character to its Ctrl-transformed byte, using the standard C0 control code mapping:

```rust
fn apply_ctrl_transform(base: &str) -> Option<String> {
    if base.len() != 1 {
        return None;
    }
    let ch = base.as_bytes()[0];
    let ctrl_char: Option<u8> = match ch {
        b'a'..=b'z' => Some(ch - b'a' + 1),   // a=0x01 .. z=0x1A
        b'A'..=b'Z' => Some(ch - b'A' + 1),
        b'@' => Some(0x00),                     // Ctrl+@ = NUL
        b'[' => Some(0x1B),                     // Ctrl+[ = ESC
        b'\\' => Some(0x1C),                    // Ctrl+\ = FS
        b']' => Some(0x1D),                     // Ctrl+] = GS
        b'^' => Some(0x1E),                     // Ctrl+^ = RS
        b'_' => Some(0x1F),                     // Ctrl+_ = US
        b'?' => Some(0x7F),                     // Ctrl+? = DEL
        _ => None,
    };
    ctrl_char.map(|c| String::from(c as char))
}
```

### 2. Event Rewriting — `rio-window/src/platform_impl/macos/view.rs`

`replace_event` now unconditionally rewrites the event when Ctrl+Option are pressed together, bypassing the `option-as-alt` check. When Ctrl is held, `characters` is set to the Ctrl-transformed base character while `charactersIgnoringModifiers` keeps the plain base character:

```rust
fn replace_event(event: &NSEvent, option_as_alt: OptionAsAlt) -> Retained<NSEvent> {
    let ev_mods = event_mods(event).state;

    // When Ctrl+Option is pressed together, always strip Option's character
    // composition regardless of the `option-as-alt` setting.
    let ctrl_with_alt =
        ev_mods.control_key() && ev_mods.alt_key() && !ev_mods.super_key();

    let ignore_alt_characters = ctrl_with_alt
        || (match option_as_alt {
            OptionAsAlt::OnlyLeft if lalt_pressed(event) => true,
            OptionAsAlt::OnlyRight if ralt_pressed(event) => true,
            OptionAsAlt::Both if ev_mods.alt_key() => true,
            _ => false,
        } && !ev_mods.super_key());

    if ignore_alt_characters {
        let ns_chars_ignoring = unsafe {
            event.charactersIgnoringModifiers()
                .expect("expected characters to be non-null")
        };

        // Re-apply Ctrl transformation since charactersIgnoringModifiers
        // strips ALL modifiers including Ctrl.
        let ns_chars = if ev_mods.control_key() {
            let base = ns_chars_ignoring.to_string();
            match apply_ctrl_transform(&base) {
                Some(ctrl) => NSString::from_str(&ctrl),
                None => ns_chars_ignoring.copy(),
            }
        } else {
            ns_chars_ignoring.copy()
        };

        // Rewritten event:
        //   characters = Ctrl-transformed char (e.g. "\x10")
        //   charactersIgnoringModifiers = base char (e.g. "p")
        NSEvent::keyEventWithType_...(&ns_chars, &ns_chars_ignoring, ...)
    } else {
        event.copy()
    }
}
```

### 3. ESC Prefix — `frontends/rioterm/src/screen/mod.rs`

`alt_send_esc()` now always returns true when Ctrl+Option are pressed together, bypassing the `option-as-alt` check:

```rust
#[cfg(target_os = "macos")]
let alt_send_esc = {
    let mods = self.modifiers.state();
    let option_as_alt = &self.renderer.option_as_alt;

    // When Ctrl+Option is pressed together, always treat Option as Alt.
    let ctrl_with_alt = mods.alt_key() && mods.control_key();

    ctrl_with_alt
        || (mods.alt_key()
            && (option_as_alt == "both"
                || (option_as_alt == "left"
                    && self.modifiers.lalt_state() == ModifiersKeyState::Pressed)
                || (option_as_alt == "right"
                    && self.modifiers.ralt_state() == ModifiersKeyState::Pressed)))
};
```

### 4. No Other Changes Required

- **`create_key_event`** (`rio-window/src/platform_impl/macos/event.rs`): No changes. When Ctrl is held, logical key already falls through to `get_logical_key_char()` which reads `charactersIgnoringModifiers` — now correctly set to the base character.
- **`process_key_event`** (`frontends/rioterm/src/screen/mod.rs`): No changes. The non-kitty path already prepends ESC when `mods.alt_key()` is true and appends `text_with_all_modifiers` bytes.

## Data Flow (Ctrl+Option+P, default config)

```
User presses: Ctrl + Option + P
                    |
                    v
1. replace_event(): ctrl_with_alt = true → always rewrite
   - charactersIgnoringModifiers() = "p"
   - apply_ctrl_transform("p") = "\x10"
   - Rewritten: characters = "\x10", charactersIgnoringModifiers = "p"
                    |
                    v
2. create_key_event():
   - text_with_all_modifiers = "\x10" (from characters())
   - logical_key = Key::Character("p") (Ctrl path → get_logical_key_char)
                    |
                    v
3. process_key_event():
   - alt_send_esc(): ctrl_with_alt = true → returns true → ALT preserved
   - should_build_sequence(): text not empty → false (non-kitty path)
   - mods.alt_key() = true → push 0x1B (ESC)
   - extend with "\x10" bytes
   - PTY receives: [0x1B, 0x10] = ESC + Ctrl-P  ✓
```

## Affected Key Combinations

All Ctrl+Option+letter combinations now work with default config (`option-as-alt = "none"`):

| Keys Pressed | Before (broken) | After (fixed) |
|---|---|---|
| Ctrl+Option+P | empty/garbage | `\x1b\x10` (ESC + Ctrl-P) |
| Ctrl+Option+N | empty/garbage | `\x1b\x0e` (ESC + Ctrl-N) |
| Ctrl+Option+F | empty/garbage | `\x1b\x06` (ESC + Ctrl-F) |
| Ctrl+Option+B | empty/garbage | `\x1b\x02` (ESC + Ctrl-B) |
| Ctrl+Option+A | empty/garbage | `\x1b\x01` (ESC + Ctrl-A) |
| Ctrl+Option+E | empty/garbage | `\x1b\x05` (ESC + Ctrl-E) |
| Ctrl+Option+U | empty/garbage | `\x1b\x15` (ESC + Ctrl-U) |

## Design Decision: Why Bypass option-as-alt for Ctrl+Option

On macOS, the Option key serves dual purpose: macOS character composition (e.g., Option+P = π) and terminal Alt modifier. The `option-as-alt` setting lets users choose which behavior they want for Option-only presses.

However, when Ctrl is also held, the macOS composition is meaningless — Ctrl+π has no defined behavior. Every terminal emulator (Ghostty, iTerm2, Alacritty) treats Ctrl+Option as Ctrl+Alt in this case. Requiring users to configure `option-as-alt` just to get Ctrl+Option working is unnecessary friction and leads to workarounds like manual byte bindings.

The `!ev_mods.super_key()` guard is retained for Cmd+Option combinations since those are used by macOS system shortcuts.

## Files Changed

| File | Change |
|---|---|
| `rio-window/src/platform_impl/macos/view.rs` | Added `apply_ctrl_transform()`. Modified `replace_event()` to unconditionally rewrite events when Ctrl+Option are held, and re-apply Ctrl transformation to the base character. |
| `frontends/rioterm/src/screen/mod.rs` | Modified `alt_send_esc()` to always return true when Ctrl+Option are held together. |

## Dependencies

- No new dependencies
- No configuration changes
- Works with default config — no `option-as-alt` setting required
- Existing manual byte bindings (e.g. `{ key = "p", with = "control | alt", bytes = [27, 16] }`) can be removed from user configs
