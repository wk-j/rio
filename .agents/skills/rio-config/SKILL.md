# Skill: Rio Configuration File

## When to Use

Use this skill when reading, editing, or migrating the user's Rio terminal
configuration file. This includes config changes needed after CR implementation,
adding new keybindings, changing themes, or troubleshooting config issues.

## Config File Location

The configuration file is always at:

```
~/.config/rio/config.toml
```

Do not scan other paths. Do not guess. Always read from this exact location.

## Config File Format

Rio uses TOML. Top-level sections match the `Config` struct in
`rio-backend/src/config/mod.rs`. Key sections:

| TOML Section | Rust Struct | Source File |
|---|---|---|
| `[renderer]` | `Renderer` | `config/renderer.rs` |
| `[window]` | `Window` | `config/window.rs` |
| `[window.side-align]` | `SideAlign` | `config/window.rs` |
| `[window.stack-align]` | `StackAlign` | `config/window.rs` |
| `[window.border-glow]` | `BorderGlow` | `config/window.rs` |
| `[fonts]` | `SugarloafFonts` | sugarloaf font types |
| `[navigation]` | `Navigation` | `config/navigation.rs` |
| `[cursor]` | `CursorConfig` | `config/mod.rs` |
| `[bindings]` | `KeyBindings` | `config/bindings.rs` |
| `[shell]` | `Shell` | `config/mod.rs` |
| `[editor]` | `Program` | `config/mod.rs` |
| `[command-overlay]` | `CommandOverlayConfig` | `config/mod.rs` |
| `[hints]` | `Hints` | `config/hints.rs` |
| `[distortion]` | `Distortion` | `config/distortion.rs` |
| `[leader]` | `LeaderConfig` | `config/mod.rs` |
| `[sound-effects]` | `SoundEffects` | `config/sound_effects.rs` |

## Field Naming Convention

TOML keys use `kebab-case`. Rust struct fields use `snake_case` with
`#[serde(rename = "kebab-case")]` or `#[serde(rename_all = "kebab-case")]`.

Example: `keyboard-only-focus` in TOML → `keyboard_only_focus` in Rust.

## Workflow: Updating Config After a CR

1. Read the current config from `~/.config/rio/config.toml`
2. Read the CR's Configuration Reference section to see the new TOML structure
3. Identify which old fields need to be migrated to new sections
4. Preserve the user's existing values — do not reset to defaults
5. Remove old fields that no longer exist in the Rust struct
6. Add new sections/fields with the user's values or sensible defaults
7. Preserve comments and section ordering style

## Workflow: Verifying Config Against Code

When in doubt whether a TOML key is valid, check the corresponding Rust struct:

1. Find the struct in `rio-backend/src/config/` that maps to the TOML section
2. Check field names, types, and `#[serde(rename = "...")]` attributes
3. Check `Default` impl for default values
4. Check `#[serde(default = "...")]` for default function names
