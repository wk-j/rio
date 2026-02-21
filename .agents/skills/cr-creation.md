# Skill: CR (Change Request) Document Creation

## When to Use

Use this skill when creating a new CR document in `docs/cr/`. CRs are detailed
design documents that fully specify a feature or fix before implementation begins.
They serve as the single source of truth for what to build, why, and how.

## Naming and Numbering

Files are named `docs/cr/NNN-short-description.md` where NNN is the next available
number (zero-padded to 3 digits). Check existing files to find the next number:

```
docs/cr/001-auto-window-alignment.md
docs/cr/002-ctrl-option-key-combinations.md
...
docs/cr/009-command-overlay-panel.md
docs/cr/010-your-new-feature.md        <- next
```

If two CRs share the same number (e.g., two variants of CR-003 exist in this repo),
that's acceptable — the short description disambiguates.

## Required Header

Every CR starts with this exact header format:

```markdown
# CR-NNN: Title in Sentence Case

**Status:** Proposed
**Date:** YYYY-MM-DD
**Author:** <author>
```

Status values: `Proposed` (not yet implemented) or `Implemented` (done).

## Document Structure

Every CR follows this section order. All sections are required unless marked optional.

### 1. Summary (required)

One paragraph. State what the feature does in concrete terms. Include the user-facing
behavior, the mechanism (what infrastructure it uses), and any key constraints.

Good: "Implement a command overlay panel — a floating, click-through panel that
spawns a command in a real PTY and renders its live terminal output as an overlay
on top of the terminal content."

Bad: "Add overlay support." (too vague — doesn't explain mechanism or constraints)

### 2. Motivation (required)

Numbered list of 3-6 concrete reasons. Each reason should be a specific use case
or problem, not abstract goals. Reference other terminals (Ghostty, iTerm2, Alacritty)
for compatibility arguments. Reference existing Rio infrastructure when the feature
reuses it.

Pattern:
```markdown
## Motivation

1. **Use case name**: Specific scenario description
2. **Technical need**: What problem this solves
3. **Reuse opportunity**: What existing infrastructure it leverages
```

### 3. Architecture (required)

ASCII diagrams showing data flow between components. Use box-drawing characters
and arrows. Always show:

- **Entry points**: Where user input enters the system
- **Data flow**: How data moves between crates (rio-backend -> rioterm -> sugarloaf)
- **State storage**: Where the new state lives
- **Render pipeline**: Where the new element renders relative to existing passes

Two common diagram types appear in Rio CRs:

**Data flow diagram** (used in every CR):
```
User action → Screen dispatch → ContextManager → ContextGrid → State change
                                                                    |
                                                                    v
Renderer::run() reads state → builds Quad/RichText → Sugarloaf renders
```

**Render pipeline diagram** (used when adding visual elements):
```
Renderer::run()                    Sugarloaf::render()
┌─────────────────────┐           ┌──────────────────────────┐
│ Build objects...     │           │ Main pass (LoadOp::Clear)│
│ Set overlays...      │──────────│ Overlay pass (LoadOp::Load)│
└─────────────────────┘           └──────────────────────────┘
```

### 4. Root Cause / Design (conditional)

**For bug fixes:** Use "## Root Cause" or "## Root Cause Analysis". Number each
contributing problem as "### Problem 1:", "### Problem 2:", etc. Show the broken
code with inline comments explaining what's wrong. Reference exact file paths and
function names.

**For features:** Use "## Design" or fold design details into "## Architecture".
Define new structs and enums with full Rust code blocks including doc comments,
derives, and field types. Explain design decisions with "### Why..." subsections
when the choice is non-obvious.

### 5. Implementation Details (required)

Numbered subsections, one per logical change unit. Each subsection:

1. Names the file being changed
2. Shows the exact code to add/modify as a Rust code block
3. Uses inline comments to explain non-obvious logic
4. Follows the actual code style (see AGENTS.md)

Pattern:
```markdown
### 1. Descriptive Name — `path/to/file.rs`

Brief explanation of what this change does and why.

\```rust
// path/to/file.rs
pub struct NewThing {
    pub field: Type,  // explanation
}
\```
```

Order implementation sections bottom-up through the stack:
1. Backend types (config, state, enums) in `rio-backend/`
2. Sugarloaf primitives (GPU structs, render passes) in `sugarloaf/`
3. Context/state management in `frontends/rioterm/src/context/`
4. Renderer integration in `frontends/rioterm/src/renderer/`
5. Screen/application wiring in `frontends/rioterm/src/screen/`
6. Bindings/actions in `frontends/rioterm/src/bindings/`

### 6. Data Flow (optional but recommended)

A step-by-step trace of a concrete scenario showing the exact bytes, function
calls, and state transitions. Use numbered steps with arrows. Name exact functions
and variables. Show intermediate values.

Example format:
```markdown
## Data Flow (Ctrl+Option+P, default config)

1. replace_event(): ctrl_with_alt = true → always rewrite
   - charactersIgnoringModifiers() = "p"
   - apply_ctrl_transform("p") = "\x10"
2. create_key_event(): text_with_all_modifiers = "\x10"
3. process_key_event(): PTY receives [0x1B, 0x10]  ✓
```

### 7. Files Changed (required)

A markdown table listing every file touched. Use "NEW" for new files.

```markdown
## Files Changed

| File | Change |
|------|--------|
| `rio-backend/src/config/feature.rs` | **NEW** — Config struct with fields X, Y, Z |
| `rio-backend/src/config/mod.rs` | Add `pub mod feature`, import, field in Config |
| `frontends/rioterm/src/renderer/mod.rs` | Add feature rendering in run() |
```

Be specific about what changes in each file — not just "modified" but what was
added, removed, or changed.

### 8. Dependencies (required)

List prerequisite CRs by number and name. List any new crate dependencies.
If there are no new dependencies, explicitly state:

```markdown
## Dependencies

- No new crate dependencies
- Uses existing infrastructure: [list what's reused]
```

Or with CR dependencies:
```markdown
## Dependencies

- CR-007 (overlay architecture design)
- CR-008 (batched overlay rendering fix)
- Existing Quick Terminal infrastructure
```

### 9. Testing (required)

Split into subsections by test type:

**For features with visual output:**
```markdown
## Testing

### Visual Verification
1. Specific thing to visually check
2. Another visual check

### Click-Through Verification (if overlay)
- Specific interaction to test

### Performance Verification
1. Check with 0 items → no overhead
2. Check with N items → acceptable
```

**For bug fixes:**
```markdown
## Testing

### Manual Testing
- Step-by-step reproduction with expected result

### Integration Tests
- Which existing tests cover the fix
```

**For features with shell integration:**
Include shell config snippets for zsh, bash, and fish.

### 10. Configuration Reference (optional)

Full TOML examples showing every configurable option with comments. Include
both key bindings and config sections.

```markdown
## Configuration Reference

### Key Bindings
\```toml
[bindings]
keys = [
  { key = "t", with = "super | shift", action = "overlay(top)" },
]
\```

### Appearance
\```toml
[command-overlay]
x = 0.6
opacity = 0.95
\```
```

### 11. Future Work / Future Enhancements (optional)

Numbered list of follow-up items. Keep brief — these are signposts, not specs.

### 12. References (optional)

Links to external documentation, upstream PRs, protocol specs, or related
terminal implementations.

## Style Guidelines

### Code Blocks

- Always specify the language: ` ```rust `, ` ```toml `, ` ```bash `, ` ```wgsl `
- Include file path as a comment on the first line: `// path/to/file.rs`
- Use the project's actual code style (4-space indent, 90-char max width)
- Include doc comments (`///`) on public items in code examples
- Show derive macros in the order used by the project:
  `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]`

### ASCII Diagrams

- Use box-drawing for data flow: `┌─┐ │ └─┘ ├ ┤ ─ →`
- Use simple boxes for pipeline diagrams
- Label each box with the actual Rust struct or function name
- Show arrows with `→`, `──▶`, or `│` + `▼` for vertical flow

### Tables

- Use markdown tables for structured data (config fields, file changes,
  key mappings, visual states)
- Always include a header row
- Align pipes for readability

### Tone

- Technical and precise. Name exact functions, files, types.
- No hedging ("should probably", "might want to"). Be definitive.
- Use "we" or imperative mood ("Add field X", "Modify function Y").
- Explain *why* for non-obvious decisions with "### Why..." subsections.

## CR Categories and What to Emphasize

| Category | Key Sections | Examples |
|----------|-------------|----------|
| **New feature** | Architecture, Design (structs), Implementation, Config | CR-001, CR-004, CR-005, CR-007, CR-009 |
| **Bug fix** | Root Cause Analysis (numbered problems), Data Flow (before/after) | CR-002, CR-003 |
| **Rendering feature** | Render pipeline placement, Quad/RichText properties, Performance | CR-004, CR-007, CR-008 |
| **Input/keyboard fix** | Keyboard pipeline data flow, byte-level traces, platform specifics | CR-002 |
| **Overlay/visual** | SugarState field, public API, render pass order, click-through | CR-007, CR-008, CR-009 |
| **GPU/shader** | WGSL code, pipeline creation, bind groups, buffer management | CR-003 |

## Completeness Checklist

Before finalizing a CR, verify:

- [ ] Summary is one concrete paragraph (not vague)
- [ ] Motivation has 3+ numbered reasons with specific use cases
- [ ] Architecture has at least one ASCII data flow diagram
- [ ] Implementation Details has numbered subsections with file paths and code
- [ ] Code blocks use correct language tags and project code style
- [ ] Files Changed table lists every file with specific changes
- [ ] Dependencies lists CR prerequisites and crate deps (or "none")
- [ ] Testing section has concrete verification steps
- [ ] Config reference (if user-facing settings are added)
- [ ] No TODO/TBD placeholders — every section is complete

## Research Before Writing

Before writing the CR, understand the existing codebase:

1. **Read related code** — find the files you'll list in "Files Changed" and
   understand their current structure
2. **Check existing patterns** — find a similar feature already implemented and
   follow its pattern (e.g., overlay → look at vi_mode_overlay, progress_bar;
   config → look at an existing `[section]` in config/mod.rs)
3. **Read dependency CRs** — if building on earlier work, read those CRs first
4. **Trace the pipeline** — for rendering features, trace the render pass order
   in `sugarloaf/src/sugarloaf.rs` to know where your pass fits
5. **Check the event system** — for cross-thread features, look at `RioEvent`
   variants in `rio-backend/src/event/mod.rs`
