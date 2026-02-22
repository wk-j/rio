# CR-011: Sound Effect System

**Status:** Proposed
**Date:** 2026-02-22
**Author:** wk

## Summary

Add a configurable sound effect system that plays short audio clips in response to terminal events (window create/close, bell, tab operations, keyboard typing, etc.). Each event maps directly to one or more sound file paths in the `[sound-effects]` TOML config section. Keyboard sounds support multiple variants per event (for natural variation when typing). Audio playback uses the `rodio` crate (enabled by a new `sound-effects` feature) and runs on a dedicated background thread with a mixer for concurrent playback, avoiding blocking the event loop.

The concept of event-driven sound effects is inspired by Opera GX's mod system, but Rio uses its own simple TOML-based configuration rather than implementing Opera GX manifest compatibility.

## Motivation

1. **User-customizable audio feedback**: Users want audible confirmation for actions like opening/closing windows, tabs, and splits. This makes the terminal feel more responsive and engaging.

2. **Simple configuration**: Sound effects are configured entirely through Rio's existing TOML config. Each event maps to a file path or a list of file paths — no external manifest format, no mod loader, no extra abstraction layers.

3. **Reuse of existing audio infrastructure**: Rio already has a bell-sound implementation (system beep on macOS/Windows, CPAL-generated sine wave on Linux). The new system extends that infrastructure with file-based playback.

4. **Progressive enhancement**: The feature is entirely optional; if the `sound-effects` feature is disabled or no sound files are configured, the terminal silently ignores playback requests. No change to the core terminal logic is required.

## Architecture

### Data Flow

```
User action / terminal event
         │
         ▼
Screen/Application dispatcher
         │
         ├── Application-level events (bell, window create/close):
         │     Application calls SoundManager::play(event) directly
         │
         └── Screen-level events (keyboard, tab, split):
               Screen sends RioEvent::PlaySound(SoundEvent) via EventProxy
               → Application receives event → SoundManager::play(event)
         │
         ▼
SoundManager::play(event)
         │
         ▼
Lookup cached audio buffer for event
         │
         ▼
Load decoded audio source (cached as Arc<CachedSound>)
         │
         ▼
rodio::OutputStreamHandle::play_raw() → mixer → cpal → audio device
         │
         ▼
Sound plays concurrently with any other active sounds
```

### Configuration Flow

```
TOML config
    │
    └── [sound-effects]
         enabled = true
         volume = 0.7
         keyboard-enabled = true
         bell = "~/.config/rio/sounds/bell.wav"
         window-create = "~/.config/rio/sounds/new_tab.mp3"
         window-close = "~/.config/rio/sounds/close_tab.mp3"
         key-letter = [
             "~/.config/rio/sounds/key1.wav",
             "~/.config/rio/sounds/key2.wav",
             "~/.config/rio/sounds/key3.wav",
         ]
         key-enter = "~/.config/rio/sounds/enter.wav"
         key-space = "~/.config/rio/sounds/space.wav"
         key-backspace = "~/.config/rio/sounds/backspace.wav"

Rio loads config
    │
    └── Build event → file path(s) mapping from config fields
         │
         ▼
    SoundManager initialized with mapping
```

### SoundManager State

```rust
use std::sync::Arc;

/// Cached decoded audio data with its original sample rate and channel count.
struct CachedSound {
    samples: Arc<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
}

pub struct SoundManager {
    /// Cached decoded audio, keyed by event; each event can have
    /// multiple variants (e.g., multiple keyboard sounds).
    cache: HashMap<SoundEvent, Vec<CachedSound>>,
    /// Rodio output stream (must be kept alive).
    _stream: rodio::OutputStream,
    /// Handle used to play sounds concurrently via the mixer.
    stream_handle: rodio::OutputStreamHandle,
    /// Event → file path mapping from config.
    mapping: HashMap<SoundEvent, Vec<PathBuf>>,
    /// Round-robin indices for variant selection.
    indices: HashMap<SoundEvent, usize>,
    /// Global volume (0.0–1.0).
    volume: f32,
}

impl SoundManager {
    /// Attempt to create a SoundManager. Returns `None` if the audio
    /// device is unavailable (e.g., headless server, no sound card).
    pub fn new(
        mapping: HashMap<SoundEvent, Vec<PathBuf>>,
        volume: f32,
    ) -> Option<Self> {
        let (_stream, stream_handle) =
            rodio::OutputStream::try_default()
                .map_err(|e| {
                    tracing::warn!(
                        "Failed to open audio device, \
                         sound effects disabled: {e}"
                    );
                    e
                })
                .ok()?;
        Some(Self {
            cache: HashMap::new(),
            _stream,
            stream_handle,
            mapping,
            indices: HashMap::new(),
            volume,
        })
    }

    pub fn play(&mut self, event: SoundEvent) {
        if let Some(buffers) = self.cache.get(&event) {
            if buffers.is_empty() {
                return;
            }
            let idx = self.indices.entry(event).or_insert(0);
            let sound = &buffers[*idx];
            *idx = (*idx + 1) % buffers.len();

            let source = rodio::buffer::SamplesBuffer::new(
                sound.channels,
                sound.sample_rate,
                sound.samples.as_ref().clone(),
            )
            .amplify(self.volume);

            // play_raw() mixes concurrently — multiple sounds can
            // overlap without queuing.
            let _ = self.stream_handle.play_raw(
                source.convert_samples(),
            );
        }
    }
}
```

**Key design decisions:**
- `OutputStreamHandle::play_raw()` feeds into rodio's internal mixer, allowing multiple sounds to play simultaneously. This avoids the sequential queuing problem of a single `Sink`.
- Each `CachedSound` stores the original sample rate and channel count from decoding, avoiding hardcoded assumptions.
- Buffers use `Arc<Vec<f32>>` so cloning for playback is cheap (reference count increment, not a full copy).
- `SoundManager::new()` returns `Option<Self>` — if the audio device is unavailable, the system degrades gracefully and `Application` stores `None`.

## Design

### Config Types (`rio-backend/src/config/sound_effects.rs`)

```rust
// rio-backend/src/config/sound_effects.rs

use std::path::PathBuf;
use serde::Deserialize;

/// A sound entry can be a single path or a list of paths (variants).
/// When multiple paths are provided, they are rotated via round-robin.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum SoundPaths {
    Single(PathBuf),
    Multiple(Vec<PathBuf>),
}

impl SoundPaths {
    pub fn into_vec(self) -> Vec<PathBuf> {
        match self {
            SoundPaths::Single(p) => vec![p],
            SoundPaths::Multiple(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SoundEffects {
    /// Explicit sound file paths per event.
    /// Each field accepts a single path or a list of paths.
    #[serde(default)]
    pub bell: Option<SoundPaths>,
    #[serde(default)]
    pub window_create: Option<SoundPaths>,
    #[serde(default)]
    pub window_close: Option<SoundPaths>,
    #[serde(default)]
    pub tab_create: Option<SoundPaths>,
    #[serde(default)]
    pub tab_close: Option<SoundPaths>,
    #[serde(default)]
    pub split_create: Option<SoundPaths>,
    #[serde(default)]
    pub split_close: Option<SoundPaths>,
    #[serde(default)]
    pub key_letter: Option<SoundPaths>,
    #[serde(default)]
    pub key_enter: Option<SoundPaths>,
    #[serde(default)]
    pub key_space: Option<SoundPaths>,
    #[serde(default)]
    pub key_backspace: Option<SoundPaths>,

    /// Global volume multiplier (0.0–1.0).
    #[serde(default = "default_volume")]
    pub volume: f32,

    /// Whether to play sounds at all.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Whether to play keyboard typing sounds (default off).
    #[serde(default = "default_keyboard_enabled")]
    pub keyboard_enabled: bool,

    /// Maximum duration in seconds for any single sound file.
    /// Files exceeding this are skipped during loading.
    #[serde(default = "default_max_duration")]
    pub max_duration: f32,
}

fn default_volume() -> f32 { 0.7 }
fn default_enabled() -> bool { true }
fn default_keyboard_enabled() -> bool { false }
fn default_max_duration() -> f32 { 5.0 }

impl Default for SoundEffects {
    fn default() -> Self {
        Self {
            bell: None,
            window_create: None,
            window_close: None,
            tab_create: None,
            tab_close: None,
            split_create: None,
            split_close: None,
            key_letter: None,
            key_enter: None,
            key_space: None,
            key_backspace: None,
            volume: default_volume(),
            enabled: default_enabled(),
            keyboard_enabled: default_keyboard_enabled(),
            max_duration: default_max_duration(),
        }
    }
}
```

### Example Config

Minimal — just a bell sound:

```toml
[sound-effects]
bell = "~/.config/rio/sounds/bell.wav"
```

Full config with keyboard variants:

```toml
[sound-effects]
enabled = true
volume = 0.5
keyboard-enabled = true

bell = "~/.config/rio/sounds/bell.wav"
window-create = "~/.config/rio/sounds/new_tab.mp3"
window-close = "~/.config/rio/sounds/close_tab.mp3"
tab-create = "~/.config/rio/sounds/new_tab.mp3"
tab-close = "~/.config/rio/sounds/close_tab.mp3"
split-create = "~/.config/rio/sounds/switch_on.mp3"
split-close = "~/.config/rio/sounds/switch_off.mp3"

# Multiple variants — rotated round-robin on each keypress
key-letter = [
    "~/.config/rio/sounds/key1.wav",
    "~/.config/rio/sounds/key2.wav",
    "~/.config/rio/sounds/key3.wav",
    "~/.config/rio/sounds/key4.wav",
]
key-enter = "~/.config/rio/sounds/enter.wav"
key-space = "~/.config/rio/sounds/space.wav"
key-backspace = "~/.config/rio/sounds/backspace.wav"
```

### File Organization

#### Directory Layout

Sound files can live anywhere on disk. A recommended layout:

```
~/.config/rio/
├── rio.toml
└── sounds/
    ├── bell.wav
    ├── new_tab.mp3
    ├── close_tab.mp3
    ├── switch_on.mp3
    ├── switch_off.mp3
    ├── key1.wav
    ├── key2.wav
    ├── key3.wav
    ├── key4.wav
    ├── enter.wav
    ├── space.wav
    └── backspace.wav
```

Users are free to organize files however they like — the config points to each file directly. There is no required directory structure or manifest format.

#### Path Resolution

- Paths starting with `~` are expanded to the user's home directory.
- Absolute paths are used as-is.
- Relative paths are resolved relative to the config directory (`~/.config/rio/`).

#### Supported Audio Formats

`rodio::Decoder` supports the following formats:

| Format | Extensions | Notes |
|--------|-----------|-------|
| WAV    | `.wav`    | Recommended — lowest decode overhead |
| MP3    | `.mp3`    | Common, good compatibility |
| OGG Vorbis | `.ogg` | Good compression, low latency |
| FLAC   | `.flac`   | Lossless, larger files |

Unsupported file extensions or corrupt files are skipped during loading with a `tracing::warn!()` message identifying the file path and error.

#### File Constraints

- **Max duration**: Controlled by `sound-effects.max-duration` (default 5.0 seconds). Files exceeding this are skipped at load time. This prevents accidental loading of music tracks or long audio files that would consume excessive memory.
- **Max file size**: No explicit limit beyond duration — decoded PCM size is bounded by duration × sample rate × channels × 4 bytes. A 5-second stereo 48 kHz file decodes to ~1.9 MB, which is acceptable.
- **Missing files**: If a config entry references a file that does not exist on disk, the entry is skipped with a `tracing::warn!()`. The event maps to no sound (not an error).
- **Permissions**: Files must be readable by the Rio process. Permission errors are logged and the file is skipped.

### Sound Selection Logic

Events that specify multiple file paths (e.g., `key-letter` with 4 WAV files) rotate through variants using **round-robin**: a per-event index increments modulo the variant count. This guarantees even distribution and requires no additional dependencies.

Events with a single file path always play that file.

### Sound Event Enum (`rio-backend/src/event/mod.rs`)

```rust
// rio-backend/src/event/mod.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundEvent {
    Bell,
    WindowCreate,
    WindowClose,
    TabCreate,
    TabClose,
    SplitCreate,
    SplitClose,
    KeyLetter,
    KeyEnter,
    KeySpace,
    KeyBackspace,
}
```

A new `RioEvent` variant is added to transport sound events from Screen to Application:

```rust
// Added to the existing RioEvent enum
PlaySound(SoundEvent),
```

### Integration with Existing Audio Bell

The existing `handle_audio_bell()` in `application.rs` will be extended:

```rust
fn handle_audio_bell(&mut self) {
    // If sound_effects has a bell sound, play it.
    if let Some(ref mut mgr) = self.sound_manager {
        if mgr.has_sound(SoundEvent::Bell) {
            mgr.play(SoundEvent::Bell);
            return;
        }
    }

    // Otherwise fall back to the platform-specific system beep
    #[cfg(target_os = "macos")] { unsafe { NSBeep(); } }
    #[cfg(target_os = "windows")] { /* MessageBeep */ }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))] {
        #[cfg(feature = "audio")]
        { /* existing sine-wave generation */ }
    }
}
```

## Implementation Details

### 1. Config Struct and Serialization — `rio-backend/src/config/sound_effects.rs`

Create the new module with the `SoundEffects` and `SoundPaths` types as shown above. Add `pub mod sound_effects;` in `config/mod.rs` and include the field in the main `Config` struct:

```rust
#[serde(default = "SoundEffects::default")]
pub sound_effects: SoundEffects,
```

### 2. Mapping Builder — `rio-backend/src/config/sound_effects.rs`

Add a method `SoundEffects::build_mapping()` that converts the config fields into a `HashMap<SoundEvent, Vec<PathBuf>>`. Each `Option<SoundPaths>` field maps to a `SoundEvent` key. `None` fields are omitted. Paths are resolved (tilde expansion, relative-to-config-dir). Files exceeding `max_duration` are skipped with a warning log.

### 3. SoundManager — `frontends/rioterm/src/sound/mod.rs`

New module that:
- Holds a `rodio::OutputStream` (kept alive) and `rodio::OutputStreamHandle`.
- Caches decoded audio buffers as `HashMap<SoundEvent, Vec<CachedSound>>` (multiple variants per event), where each `CachedSound` stores `Arc<Vec<f32>>`, `sample_rate: u32`, and `channels: u16`.
- Maintains round-robin indices (`HashMap<SoundEvent, usize>`) for variant selection.
- Provides `play(&mut self, event: SoundEvent)` which picks the next variant and plays it via `stream_handle.play_raw()` for concurrent mixing.
- Provides `has_sound(&self, event: SoundEvent) -> bool` for checking availability.
- Returns `Option<Self>` from `new()` — gracefully handles missing audio devices.

Decoding uses `rodio::Decoder` (supports WAV, MP3, OGG, FLAC). The original sample rate and channel count are preserved from the decoder. Each variant is decoded once and kept in memory via `Arc`.

### 4. Application Integration — `frontends/rioterm/src/application.rs`

- Add a `sound_manager: Option<SoundManager>` field to `Application`.
- Initialize it in `Application::new()` if `config.sound_effects.enabled` and the `sound-effects` feature is enabled. Uses `SoundManager::new()` which returns `None` on audio device failure.
- In `handle_audio_bell()`, call `sound_manager.play(SoundEvent::Bell)` if available.
- In the `RioEvent::CreateWindow` and `RioEvent::CloseWindow` handlers, call `play(SoundEvent::WindowCreate/Close)`.
- Add a handler for the new `RioEvent::PlaySound(event)` variant that delegates to `sound_manager.play(event)`.

### 5. Screen-Level Triggers — `frontends/rioterm/src/screen/mod.rs`

Screen does not hold a reference to `SoundManager`. Instead, it sends sound events through the existing `EventProxy`:

- For tab/split events triggered by `Action` variants, `Screen::process_action()` sends `RioEvent::PlaySound(SoundEvent::TabCreate)` (etc.) via the event proxy after dispatching the action.
- Tab close vs. split close is distinguished by inspecting `ContextManager` state: if the closed context is the sole pane in its tab, it is a `TabClose`; otherwise it is a `SplitClose`.

Keyboard sounds are triggered in `Screen::process_key_event()` when `config.sound_effects.keyboard_enabled` is true. The method inspects the key event:
- Letters, digits, symbols → `SoundEvent::KeyLetter`
- `NamedKey::Enter` → `SoundEvent::KeyEnter`
- `NamedKey::Space` → `SoundEvent::KeySpace`
- `NamedKey::Backspace` → `SoundEvent::KeyBackspace`

Only key-press events (`ElementState::Pressed`) trigger sounds. Key repeats are filtered using `KeyEvent::repeat` — if `key_event.repeat` is `true`, no sound is played.

### 6. Feature Gating

A new feature flag `sound-effects` is introduced, separate from the existing `audio` flag:

```toml
# frontends/rioterm/Cargo.toml
[dependencies]
rodio = { version = "0.19", optional = true, default-features = false, features = ["wav", "mp3", "vorbis", "flac"] }

[features]
default = ["wayland", "x11"]
audio = ["cpal"]                    # existing: Linux/BSD sine-wave bell
sound-effects = ["rodio"]           # new: cross-platform file-based sounds
```

Note: `rodio` is added as a **non-platform-conditional** dependency so it works on macOS, Windows, and Linux. The existing `audio` feature (Linux-only `cpal` sine-wave bell) remains unchanged.

All new sound-effect code is guarded by `#[cfg(feature = "sound-effects")]`. If the feature is disabled, `SoundManager` is not compiled and `Application` stores `sound_manager: None` unconditionally.

### 7. Key Repeat, Volume Control, and Config Reload

- **Key repeat**: Filtered by checking `key_event.repeat`. If `true`, no `RioEvent::PlaySound` is sent. This is a direct boolean check on the `KeyEvent` struct provided by `rio-window`, not a time-based debounce.
- **Volume scaling**: The `sound_effects.volume` field is passed to `SoundManager` at construction time. Each played buffer is wrapped with `rodio::Source::amplify(volume)` before being sent to the mixer.
- **Keyboard-only toggle**: The `keyboard_enabled` flag allows users to keep window/bell sounds while disabling typing sounds.
- **Config hot-reload**: When `RioEvent::UpdateConfig` is received in `Application`, the sound manager is re-initialized if `sound_effects` settings have changed. The old `SoundManager` is dropped (which stops the output stream), and a new one is created with the updated mapping and volume. The audio cache is rebuilt from scratch — this is acceptable because config reloads are infrequent.

## Data Flow Examples

### Window Create

1. User presses `Super+N` → `Action::WindowCreateNew` → `Screen::process_action()` → `RioEvent::CreateWindow` sent via event proxy.
2. `Application::handle_event()` receives `RioEventType::Rio(RioEvent::CreateWindow)`.
3. `Application` calls `self.sound_manager.as_mut().map(|m| m.play(SoundEvent::WindowCreate))`.
4. `SoundManager` looks up the `CachedSound` for the event.
5. If cache miss, loads the file via `rodio::Decoder`, stores `CachedSound` with original sample rate/channels.
6. Plays via `stream_handle.play_raw()` — sound mixes concurrently with any other active sounds.
7. Terminal continues rendering; sound plays in background.

### Keyboard Typing (Letter "A")

1. User presses `A` key → `Screen::process_key_event()` called with `KeyEvent`.
2. `key_event.repeat` is `false` and `config.sound_effects.keyboard_enabled` is `true`, key is a letter → `SoundEvent::KeyLetter`.
3. `Screen` sends `RioEvent::PlaySound(SoundEvent::KeyLetter)` via the `EventProxy`.
4. `Application` receives the event and calls `sound_manager.play(SoundEvent::KeyLetter)`.
5. `SoundManager` picks the next variant (e.g., `key3.wav`) via round-robin.
6. Buffer already cached; plays via `play_raw()` — concurrent with any other sounds.
7. Sound plays while the character is sent to the PTY (parallel, non-blocking).

### Tab Close vs. Split Close

1. User presses `Ctrl+W` → `Action::CloseTerminal` → `Screen::process_action()`.
2. Before closing, `Screen` checks `ContextManager`: if the current context is the only pane in its tab, this is a `TabClose`; otherwise it is a `SplitClose`.
3. `Screen` sends `RioEvent::PlaySound(SoundEvent::TabClose)` or `RioEvent::PlaySound(SoundEvent::SplitClose)` accordingly.
4. `Application` receives and plays the appropriate sound.

## Files Changed

| File | Change |
|------|--------|
| `rio-backend/src/config/sound_effects.rs` | **NEW** — `SoundEffects`, `SoundPaths` types, mapping builder |
| `rio-backend/src/config/mod.rs` | Add `pub mod sound_effects`, include field in `Config` |
| `rio-backend/src/event/mod.rs` | Add `SoundEvent` enum, add `RioEvent::PlaySound(SoundEvent)` variant |
| `frontends/rioterm/src/sound/mod.rs` | **NEW** — `SoundManager` with rodio integration |
| `frontends/rioterm/src/application.rs` | Add `sound_manager` field, init, playback calls, `PlaySound` handler, config reload |
| `frontends/rioterm/src/screen/mod.rs` | Send `RioEvent::PlaySound` for tab/split actions and keyboard events |
| `frontends/rioterm/Cargo.toml` | Add `rodio` dependency (non-platform-conditional), `sound-effects` feature |
| `Cargo.toml` (workspace) | Add `rodio` to workspace dependencies |

## Dependencies

- **New crate**: `rodio` (MIT/Apache-2.0) for audio playback, decoding, and mixing. Added as a cross-platform optional dependency under the `sound-effects` feature.
- **Existing infrastructure**: `RioEvent` system, config parsing, `EventProxy`.
- **Unchanged**: The existing `audio` feature and `cpal` dependency (Linux/BSD sine-wave bell) are not modified.
- **No CR dependencies** — this is a standalone feature.

## Testing

### Unit Tests

1. **Mapping builder** (`rio-backend/src/config/sound_effects.rs`):
   - `test_build_mapping_single_paths`: Config with single-path fields produces a mapping with one-element vectors.
   - `test_build_mapping_multiple_paths`: Config with `key-letter` as a list produces a multi-element vector.
   - `test_build_mapping_none_fields_omitted`: Fields set to `None` are not present in the mapping.
   - `test_empty_config`: Default `SoundEffects` produces an empty mapping.

2. **SoundPaths deserialization** (`rio-backend/src/config/sound_effects.rs`):
   - `test_sound_paths_single_string`: A TOML string deserializes to `SoundPaths::Single`.
   - `test_sound_paths_array`: A TOML array deserializes to `SoundPaths::Multiple`.
   - `test_sound_paths_into_vec`: Both variants convert correctly via `into_vec()`.

3. **Config deserialization** (`rio-backend/src/config/sound_effects.rs`):
   - `test_toml_kebab_case`: Verify that `window-create`, `key-letter`, etc. deserialize correctly.
   - `test_max_duration_default`: Verify default is 5.0s.
   - `test_volume_default`: Verify default is 0.7.
   - `test_keyboard_enabled_default_false`: Verify keyboard sounds are off by default.

4. **Round-robin selection** (`frontends/rioterm/src/sound/mod.rs`):
   - `test_round_robin_cycles`: With 3 variants, calling `play()` 6 times should select indices `[0, 1, 2, 0, 1, 2]`.
   - `test_single_variant`: With 1 variant, index always stays at 0.

5. **SoundEvent mapping** (`rio-backend/src/event/mod.rs`):
   - `test_sound_event_hash_eq`: Verify `Hash` and `Eq` work correctly for use as `HashMap` keys.

6. **Path resolution** (`rio-backend/src/config/sound_effects.rs`):
   - `test_tilde_expansion`: `"~/sounds/bell.wav"` expands to the user's home directory.
   - `test_relative_path_resolved_to_config_dir`: `"sounds/bell.wav"` resolves to `{config_dir}/sounds/bell.wav`.
   - `test_absolute_path_unchanged`: `/tmp/bell.wav` is used as-is.
   - `test_missing_file_skipped`: A path to a non-existent file produces an empty mapping for that event, not an error.

### Manual Verification

1. **Single sound**: Set `sound-effects.bell = "path/to/bell.wav"`. Trigger bell (`echo -e '\a'`) — should hear the sound.
2. **Window sounds**: Set `window-create` and `window-close` paths. Open/close windows — should hear corresponding sounds.
3. **Keyboard variants**: Set `key-letter` to a list of 4 WAV files, enable `keyboard-enabled = true`. Type letters — should hear variants rotating.
4. **Concurrent playback**: Rapidly create multiple tabs — sounds should overlap, not queue sequentially.
5. **Fallback to system bell**: Remove `sound-effects.bell` or disable `sound-effects.enabled`. Set `bell.audio = true`. Trigger bell — should produce system beep.
6. **Key repeat**: Hold down a key; only the first press should trigger sound, repeats should be silent.
7. **Feature disable**: Compile without `--features sound-effects`. All sound playback should be silently ignored.
8. **No audio device**: Test on a headless machine or with audio device removed. Verify Rio starts without errors and logs a warning.
9. **Config reload**: Change `sound-effects.volume` in `rio.toml` while Rio is running. Verify new volume takes effect after reload.

### Performance Verification

- **Memory**: Load 10 sound files (total ~5 MB). Memory increase should stay within the size of decoded PCM buffers (approx. file size × 2). Files exceeding `max_duration` (default 5.0s) are rejected.
- **Latency**: Sound playback must not block the event loop. `play_raw()` returns immediately; mixing happens on rodio's background thread.
- **Variant rotation**: Typing 20 letters should cycle through all `key-letter` variants without stuttering or sequential queuing.

## Future Work

1. **Per-key sound mapping**: Allow assigning different sounds to individual keys (e.g., WASD gaming sounds).
2. **Dynamic volume control**: Add a configurable volume slider in a future UI overlay.
3. **Random variant selection**: Add a `selection = "random"` config option as an alternative to round-robin.
4. **Opera GX mod compatibility**: Optionally load sound mappings from Opera GX `manifest.json` format for users who already have mod packs.
5. **Sound effect preview**: UI to test each sound without triggering the actual event.

## References

- Opera GX mod sound effects (concept inspiration): https://github.com/nicedeck/nicedeck-gxmods
- `rodio` documentation: https://github.com/RustAudio/rodio
- Rio's existing audio bell: `frontends/rioterm/src/application.rs`, `handle_audio_bell()`.
