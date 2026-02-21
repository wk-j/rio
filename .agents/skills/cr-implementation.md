# Skill: CR (Change Request) Implementation

## When to Use

Use this skill when implementing a feature described in a CR document from `docs/cr/`.
CRs are detailed design documents that specify architecture, data flow, file changes,
config additions, rendering pipeline integration, and testing strategy.

## CR Document Structure

Every CR follows this structure — read the entire document before writing code:

| Section | What It Tells You |
|---------|-------------------|
| **Summary** | One-paragraph overview of the feature |
| **Status** | `Proposed` (not started) or `Implemented` (done) |
| **Motivation** | Why the feature exists — understand intent before coding |
| **Architecture** | Data flow diagrams showing how components connect |
| **Design** | Struct definitions, enum variants, render pipeline placement |
| **Implementation Details** | Numbered steps with code snippets and file paths |
| **Files Changed** | Exact list of files to create or modify |
| **Dependencies** | Other CRs or infrastructure this builds on |
| **Testing** | Manual and automated verification steps |
| **Configuration Reference** | TOML config examples for user-facing settings |

## Implementation Workflow

### Phase 1: Understand the Full Picture

1. Read the CR end-to-end. Pay special attention to:
   - The **Architecture** diagrams — they show data flow between components
   - The **Files Changed** table — this is your checklist
   - The **Dependencies** section — read prerequisite CRs if referenced
   - The **Render Pass Order** — if the CR touches rendering, understand where
     the new pass fits relative to existing passes

2. Identify the CR's layer boundaries. Rio has three main layers:
   - **`rio-backend/`** — config, terminal state, ANSI parsing, crosswords (terminal grid)
   - **`frontends/rioterm/`** — screen, renderer, context management, bindings, application event loop
   - **`sugarloaf/`** — GPU rendering: quads, rich text, images, shaders, font layout

3. Check if dependent CRs are implemented by reading their Status field.

### Phase 2: Plan the Implementation Order

Work bottom-up through the stack:

1. **Backend types first** — config structs, enums, state fields in `rio-backend/`
2. **Sugarloaf primitives** — new rendering structs, GPU passes, brush methods in `sugarloaf/`
3. **Context/state management** — new state structs, lifecycle methods in `frontends/rioterm/src/context/`
4. **Renderer integration** — render loop changes in `frontends/rioterm/src/renderer/mod.rs`
5. **Screen/application wiring** — action dispatch, event handling, config hot-reload in `frontends/rioterm/src/screen/mod.rs` and `application.rs`
6. **Bindings** — new Action variants and parsing in `frontends/rioterm/src/bindings/mod.rs`

### Phase 3: Implement

For each file in the **Files Changed** table:

1. Read the existing file to understand the surrounding code
2. Follow the code snippets in the CR — they show the exact patterns to use
3. Match existing code style (see AGENTS.md for conventions)
4. Wire new state through the existing event/render pipeline

#### Common Patterns in Rio CRs

**Adding config options:**
```
rio-backend/src/config/<feature>.rs  — new module with struct + Default + Deserialize
rio-backend/src/config/mod.rs        — pub mod, import, add field to Config + Default
```
Config structs use `#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]` with
`serde(rename_all = "kebab-case")` for TOML field names. Provide sensible defaults.

**Adding terminal state:**
```
rio-backend/src/crosswords/mod.rs    — add field to Crosswords struct
frontends/rioterm/src/context/renderable.rs — expose in TerminalSnapshot
```

**Adding a new overlay/render element:**
```
sugarloaf/src/sugarloaf/state.rs     — add field to SugarState + setter
sugarloaf/src/sugarloaf.rs           — add public setter method + render pass
frontends/rioterm/src/renderer/mod.rs — build the element from terminal state
```
Overlays use `Option<Quad>` for single elements or `Vec<Object>` for compound elements.
Render passes use `LoadOp::Load` to composite over existing content.

**Adding an Action (keybinding/leader):**
```
frontends/rioterm/src/bindings/mod.rs — add variant to Action enum + parsing
frontends/rioterm/src/screen/mod.rs   — add dispatch in process_action()
```
Action parsing uses regex for parameterized actions: `overlay\(([^()]+)\)`.

**Adding events between threads:**
```
rio-backend/src/event/mod.rs          — add variant to RioEvent
frontends/rioterm/src/application.rs   — handle in user_event()
```

**Config hot-reload:**
```
frontends/rioterm/src/screen/mod.rs    — update in update_config() method
```
Copy new config values to both `context_manager.config` and each `context_grid`.

#### Rendering Pipeline Awareness

The render pipeline order matters. When the CR specifies where a new pass goes,
respect this ordering:

```
1. Main pass (LoadOp::Clear) — background image, quads, rich text
2. vi_mode_overlay (LoadOp::Load)
3. visual_bell (LoadOp::Load)
4. progress_bar (LoadOp::Load)
5. overlay_layers (LoadOp::Load) — if applicable
6. Filters (LoadOp::Load) — post-processing shaders
```

RichText content MUST go in the main pass (batch constraint — no render_single).
Quads can go in any pass via render_single() or render_slice().

#### PTY/Context Creation Pattern

For features that spawn a PTY (like command overlays or quick terminal):

1. Clone the config and override `shell` with the target command
2. Set `use_fork = false` when overriding the shell program
3. Call `ContextManager::create_context()` with the modified config
4. Allocate a `rich_text_id` via `sugarloaf.create_rich_text()` beforehand
5. Handle process exit in `should_close_context_manager()` to clean up

### Phase 4: Verify

1. **Build:** `cargo build -p rioterm` (dev) or `cargo build --release` (release)
2. **Lint:** `cargo fmt -- --check && cargo clippy --all-targets --all-features -- -D warnings`
3. **Test:** `cargo test --release` (all tests) or `cargo test -p <crate> <test_name>` (single)
4. **Visual:** If the CR involves rendering, run `make dev` and verify manually
5. Walk through the CR's **Testing** section — it lists specific verification steps

### Phase 5: Handle Cross-Cutting Concerns

- **Platform gates:** Use `#[cfg(target_os = "...")]` for platform-specific code.
  macOS is the primary target; use `#[cfg(not(target_os = "windows"))]` for Unix-shared code.
- **Click-through:** Overlays are click-through by architecture — the input pipeline
  only checks pane RichText positions in ContextGrid. Overlay RichText IDs are not
  in ContextGrid, so they are inherently non-interactive.
- **Memory:** Prefer `Option<T>` for optional single elements, `Vec<T>` for collections.
  Use `SmallVec` only where the codebase already does.

## Key Crate Boundaries

| Crate | Role | Key Files |
|-------|------|-----------|
| `rio-backend` | Config, terminal state, ANSI, crosswords grid | `config/mod.rs`, `crosswords/mod.rs`, `performer/handler.rs`, `event/mod.rs` |
| `frontends/rioterm` | Application, screen, renderer, context, bindings | `screen/mod.rs`, `renderer/mod.rs`, `context/mod.rs`, `context/grid.rs`, `bindings/mod.rs`, `application.rs` |
| `sugarloaf` | GPU rendering, quads, rich text, fonts, images | `sugarloaf.rs`, `sugarloaf/state.rs`, `components/quad/mod.rs`, `components/rich_text/mod.rs`, `layout/content.rs` |
| `copa` | VTE terminal parser | `src/lib.rs` |
| `teletypewriter` | PTY abstraction | Platform-specific files |

## Checklist Template

When starting a CR implementation, create todos from this template:

- [ ] Read entire CR document
- [ ] Read dependency CRs if any
- [ ] Implement backend types (config, state, enums)
- [ ] Implement sugarloaf primitives (if rendering changes needed)
- [ ] Implement context/state management
- [ ] Implement renderer integration
- [ ] Wire screen/application dispatch
- [ ] Add bindings/actions (if user-triggered)
- [ ] Add config hot-reload support
- [ ] Build and fix compilation errors
- [ ] Run lint (fmt + clippy)
- [ ] Run tests
- [ ] Visual verification (if rendering feature)
- [ ] Walk through CR testing section
