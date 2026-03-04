# CR-018: Add Apple Symbols to macOS Font Fallback Chain

**Status:** Proposed
**Date:** 2026-03-04
**Author:** wk

## Summary

Add "Apple Symbols" to the macOS external font fallback list so that Unicode
characters missing from the user's primary font (and from Menlo, Geneva, and
Arial Unicode MS) can be rendered using Apple's broad-coverage symbol font
instead of appearing as tofu (missing glyph boxes).

## Motivation

1. **Broken glyphs in real-world tools**: lazygit uses U+23E3 (BENZENE RING
   WITH CIRCLE) for merge commit markers in its graph view. This character is
   absent from popular terminal fonts (Mononoki, CascadiaCode, JetBrains Mono)
   and from Rio's current macOS fallbacks (Menlo, Geneva, Arial Unicode MS).
   Users see tofu boxes where Ghostty and other terminals render the glyph
   correctly because they include Apple Symbols in their fallback chain.

2. **Broad Unicode coverage**: Apple Symbols ships with every macOS install and
   covers thousands of miscellaneous symbols, technical characters, and
   mathematical operators that no monospace terminal font typically includes.
   Adding it once fixes an entire class of missing-glyph problems.

3. **Zero user configuration**: The fix is a single entry in the platform
   fallback list — users do not need to change their config or install extra
   fonts.

4. **Minimal performance impact**: Apple Symbols is only consulted when all
   preceding fonts in the fallback chain fail to provide a glyph.
   `lookup_for_font_match()` already does a linear scan through all loaded
   fonts; adding one more font to the tail of the list has negligible cost.

## Root Cause Analysis

### The character

lazygit (v0.57.0) emits U+23E3 (UTF-8 bytes `e2 8f a3`) for merge commit
nodes in its graph view when `nerdFontsVersion` is empty (icons disabled).
This is a standard Unicode 1.1 character in the Miscellaneous Technical block
(U+2300–U+23FF).

### Font coverage on macOS

| Font | Has U+23E3? | In Rio fallback chain? |
|------|-------------|----------------------|
| Mononoki Nerd Font Mono | No | Yes (user primary) |
| Menlo | No | Yes (fallback) |
| Geneva | No | Yes (fallback) |
| Arial Unicode MS | No | Yes (fallback) |
| SymbolsNerdFontMono (built-in) | No | Yes (built-in) |
| **Apple Symbols** | **Yes** | **No** ← the gap |
| STIX Two Math | Yes | No |
| .LastResort | Yes | No (system-only) |

### Font matching path

```
FontLibraryData::load()          sugarloaf/src/font/mod.rs:323
  ├─ Index 0–3: user fonts (regular, italic, bold, bold-italic)
  ├─ Index 4+:  external_fallbacks()  ← sugarloaf/src/font/fallbacks/mod.rs
  │             macOS: Menlo, Geneva, Arial Unicode MS
  ├─ Emoji font (Twemoji built-in)
  ├─ User extras
  ├─ Built-in SymbolsNerdFontMono
  └─ Symbol-map fonts

lookup_for_font_match()          sugarloaf/src/font/mod.rs:55
  → linear scan through all fonts
  → checks charmap for glyph coverage
  → returns font 0 (primary) if nothing matches ← tofu
```

When no font in the chain has U+23E3, `lookup_for_font_match` falls back to
font index 0 (the user's primary font), which also lacks the glyph, producing
a `.notdef` / tofu box.

## Architecture

No new modules, structs, or rendering changes. The fix is a single addition
to the existing platform fallback font list.

### Before

```rust
#[cfg(target_os = "macos")]
pub fn external_fallbacks() -> Vec<String> {
    vec![
        String::from("Menlo"),
        String::from("Geneva"),
        String::from("Arial Unicode MS"),
    ]
}
```

### After

```rust
#[cfg(target_os = "macos")]
pub fn external_fallbacks() -> Vec<String> {
    vec![
        String::from("Menlo"),
        String::from("Geneva"),
        String::from("Arial Unicode MS"),
        String::from("Apple Symbols"),
    ]
}
```

### Fallback chain order (after fix)

```
User primary font (e.g. Mononoki NF)
  → Menlo
    → Geneva
      → Arial Unicode MS
        → Apple Symbols          ← NEW
          → Twemoji (emoji)
            → User extras
              → SymbolsNerdFontMono (built-in)
                → Symbol-map fonts
```

Apple Symbols is placed after Arial Unicode MS and before emoji/Nerd Font
symbol fonts because:
- It should not override glyphs that monospace fonts already provide
- It covers standard Unicode symbols that neither emoji nor Nerd Font fonts
  target
- Its glyphs are proportional, so it is a last resort for standard symbols
  before falling back to specialty fonts

## Key Files

| File | Lines | Change |
|------|-------|--------|
| `sugarloaf/src/font/fallbacks/mod.rs` | 2–9 | Add `"Apple Symbols"` to macOS `external_fallbacks()` |

## Implementation Steps

1. Open `sugarloaf/src/font/fallbacks/mod.rs`
2. In the `#[cfg(target_os = "macos")]` block, add `String::from("Apple Symbols")`
   after the `"Arial Unicode MS"` entry
3. Build: `cargo build -p rioterm`
4. Run lint: `cargo fmt -- --check && cargo clippy --all-targets --all-features -- -D warnings`
5. Run tests: `cargo test --release`

## Testing

### Manual verification

1. Launch Rio with any font that lacks U+23E3 (e.g. Mononoki, CascadiaCode)
2. Run: `printf '\xe2\x8f\xa3\n'` — should render the benzene ring symbol (⏣)
   instead of a tofu box
3. Open lazygit in a repository with merge commits — graph merge nodes should
   render correctly
4. Verify that normal text, Nerd Font icons, emoji, and Powerline symbols are
   unaffected

### Automated verification

Existing `cargo test --release` covers font loading and fallback chain
construction. No new test cases are strictly required since this is a data
change (adding a font name to a list), but the build and test suite must pass
cleanly.

### Regression checks

- Confirm Menlo, Geneva, Arial Unicode MS still load (Apple Symbols is additive)
- Confirm that if Apple Symbols is not installed (non-macOS or stripped system),
  the font loader gracefully skips it (existing behavior — `load()` ignores
  fonts that fail to resolve)

## Future Considerations

- **SymbolsNerdFontMono update**: The built-in copy is v3.0.1; latest is v3.3.0.
  Updating it would improve Nerd Font icon coverage but does NOT fix U+23E3
  (which is a standard Unicode character, not a Nerd Font symbol). This could
  be a separate CR.
- **Other platforms**: Windows already has `"Segoe UI Symbol"` in its fallback
  list. Linux has `"Noto Sans Symbols"` and `"Noto Sans Symbols2"`. Both
  platforms likely cover U+23E3 already.
