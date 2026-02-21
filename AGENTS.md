# AGENTS.md — Rio Terminal

Rio is a hardware-accelerated GPU terminal emulator written in Rust.
Workspace crates: `sugarloaf` (GPU renderer), `rio-backend` (config, terminal state,
ANSI parsing), `frontends/rioterm` (application, screen, renderer, context management),
`copa` (VTE parser), `teletypewriter` (PTY), `corcovado` (event loop), `rio-window`
(windowing), `rio-proc-macros`.

## Build / Run / Test / Lint

```bash
# Toolchain: Rust 1.92 stable (see rust-toolchain.toml)
# MSRV: 1.92.0

# Build (dev)
cargo build -p rioterm

# Build (release, stripped + LTO)
cargo build --release

# Run (dev, with Metal HUD on macOS)
make dev                    # or: MTL_HUD_ENABLED=1 cargo run -p rioterm

# Run (release)
make run                    # or: cargo run -p rioterm --release

# Lint (must pass both)
cargo fmt -- --check --color always
cargo clippy --all-targets --all-features -- -D warnings

# Test — all crates (release mode, as in CI)
cargo test --release

# Test — single crate
cargo test -p rio-backend --release
cargo test -p copa --release
cargo test -p sugarloaf --release

# Test — single test by name
cargo test -p rio-backend --release test_empty_config_file
cargo test -p rio-backend --release -- config::tests::test_filepath

# Test — all tests in a module (filter by path)
cargo test -p rio-backend --release -- config::tests

# Linux-specific: install deps first
# sudo apt-get install libasound2-dev libfontconfig1-dev

# Platform feature flags (Linux only)
cargo build --no-default-features --features=x11
cargo build --no-default-features --features=wayland
```

## Project Structure

```
rio-backend/src/
  config/          # Config types, TOML deserialization, defaults
  crosswords/      # Terminal grid, scrollback, selection
  ansi/            # ANSI/VTE escape sequence types
  performer/       # Terminal state machine (handler.rs = OSC/CSI dispatch)
  event/           # RioEvent enum (inter-thread communication)
  error/           # RioError, RioErrorType

frontends/rioterm/src/
  application.rs   # Event loop, window management, event dispatch
  screen/mod.rs    # Screen struct: input handling, action dispatch, config reload
  renderer/mod.rs  # Renderer: terminal snapshot -> RichText/Quad objects
  context/mod.rs   # ContextManager: PTY contexts, tab/split lifecycle
  context/grid.rs  # ContextGrid: pane layout, quick terminal, command overlays
  bindings/mod.rs  # Action enum, keybinding parsing

sugarloaf/src/
  sugarloaf.rs     # Public API, wgpu render passes
  sugarloaf/state.rs  # SugarState: render state (quads, overlays, rich text IDs)
  components/quad/ # QuadBrush: GPU quad rendering (backgrounds, borders, shadows)
  components/rich_text/ # RichTextBrush: text shaping and glyph rendering
  layout/content.rs    # Content: text builder API, font shaping pipeline
  font/            # FontLibrary, font loading, glyph caching

docs/cr/           # Change Request documents (feature specs)
```

## Code Style

### Formatting
- **rustfmt.toml**: `max_width = 90`, `tab_spaces = 4`, `hard_tabs = false`,
  `reorder_imports = true`, `reorder_modules = true`
- Line endings: LF. Charset: UTF-8. Trailing whitespace: trimmed.
- Always run `cargo fmt` before committing.

### Imports
- Module declarations (`pub mod`, `mod`) come first, before any `use` statements.
- Then `use crate::...` (internal), then external crates, then `std::`.
- Groups are NOT separated by blank lines — they flow continuously.
- Use brace grouping for multi-item imports: `use crate::crosswords::{grid::Row, pos::Pos};`
- In test modules, always start with `use super::*;`.
- Never use `use crate::*` (bare glob). Targeted imports only.

### Naming
- Types: `PascalCase` — `ContextManager`, `CommandOverlayStyle`, `ProgressState`
- Functions/methods: `snake_case` — `toggle_command_overlay()`, `config_dir_path()`
- Constants: `SCREAMING_SNAKE_CASE` — `MIN_COLS`, `MAX_SEARCH_WHILE_TYPING`
- Modules: `snake_case` — `command_overlay`, `rich_text`, `vi_mode`
- Enum variants: `PascalCase` — `Action::ToggleCommandOverlay(String)`

### Types and Patterns
- Use `pub type` for semantic aliases: `pub type ColorArray = [f32; 4];`
- Error return types: `Result<T, Box<dyn Error>>` for complex init functions.
- Custom error enums with manual `Display`/`Error` impls — no `anyhow`, no `thiserror`.
- Config structs: `#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]` with
  `#[serde(rename_all = "kebab-case")]` for TOML compatibility.
- Generics: `pub struct Context<T: EventListener>` — trait bounds on struct definition.

### Error Handling
- Use `?` operator for propagation in fallible functions.
- Use `tracing::error!()` / `tracing::warn!()` for logging errors in production paths.
- `.unwrap()` is acceptable in tests and for "should never fail" cases.
- Prefer `.unwrap_or()` / `.unwrap_or_default()` over bare `.unwrap()` in production.

### Visibility
- Struct fields default to `pub` (the dominant pattern in this codebase).
- `pub(crate)` is rare — used mainly in `sugarloaf/src/font_introspector/`.
- Private fields for internal state that should not be accessed directly.
- Most modules are `pub mod`; private modules use `mod`.

### Logging
- Use the `tracing` crate. Prefer qualified paths: `tracing::info!()`, `tracing::error!()`.
- Named imports (`use tracing::{debug, info};`) when a file has many log calls.

### Platform-Specific Code
- Use `#[cfg(target_os = "macos")]`, `#[cfg(target_os = "windows")]`,
  `#[cfg(not(any(target_os = "windows", target_os = "macos")))]` for Linux/other.
- Inline `#[cfg]` on items/blocks — not separate platform module trees.
- macOS is the primary development target.

### Testing
- Tests live in `#[cfg(test)] mod tests { ... }` at the bottom of the file.
- Always `use super::*;` as the first import in test modules.
- Test function names: `test_` prefix with descriptive `snake_case` name.
- `assert_eq!()` is the primary assertion. Use `assert!(matches!(...))` for patterns.
- Helper functions for test setup go inside the test module.

### Derive Ordering
- `Debug` first, then `Clone`, `Copy`, `PartialEq`, `Eq`, then serde derives.
- Example: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]`

### Documentation
- `///` on public structs, enums, enum variants, constants, and public functions.
- `//!` module-level docs on select library entry points (`copa/src/lib.rs`).
- Private internals typically use `//` inline comments, not doc comments.
- License headers as `//` comments at file top (when present).

## CR (Change Request) Documents

Feature specs live in `docs/cr/NNN-feature-name.md`. They contain full architecture,
implementation details with code snippets, file change lists, and testing plans.
See `.agents/skills/cr-implementation.md` for the implementation workflow.
