# CR-017: Bookmark Tab Decorator

**Status:** Implemented
**Date:** 2026-03-02
**Author:** wk

## Summary

Implement a configurable sidebar tab indicator system for Rio's `Bookmark`
navigation mode. Each tab is represented by a small colored pill/dot quad
anchored to the right edge of the window. The active tab is visually
distinguished by height and color. The system supports per-tab hue rotation,
shadow, border, an optional deep-edge glow halo on the active pill, a
full-height vertical cutline along the right window edge, color automation
overrides, and suppression when the quick terminal overlay is active.

## Motivation

1. **Screen real estate**: Unlike `TopTab`/`BottomTab`, bookmark indicators take
   no vertical space from the terminal viewport — they float over the right edge,
   leaving the full window height available for terminal content.

2. **Glanceable tab count**: Vertical dot/pill stacks are immediately legible at a
   glance without reading text. The height difference between active and inactive
   pills provides an instant focal point.

3. **Per-tab color identity**: The optional hue-rotation mode assigns each tab a
   distinct HSL color automatically, making tab identity visually unambiguous
   without user configuration.

4. **Composable with existing systems**: The decorator reuses `color_automation`
   (program/path rules already used by TopTab/BottomTab) and the same `Quad`
   rendering pipeline used everywhere else in the renderer, requiring no new GPU
   infrastructure.

5. **Quick terminal awareness**: When the quick terminal overlay is shown, no tab
   pill appears active, avoiding a misleading highlight while the overlay captures
   focus.

6. **Edge cutline**: An optional full-height vertical line flush with the right
   window edge — colored with the active tab's pill color — mirrors the window
   border glow aesthetic and provides a persistent color accent even when the
   active pill is small or off-screen.

## User Flow

```
1.  User sets [navigation] mode = "Bookmark" in config
2.  For each open tab, a pill quad is drawn along the right edge of the window
3.  Active tab pill: taller, brighter color (tabs_active_highlight or HSL-active)
4.  Inactive tab pills: shorter, dimmer color (tabs or HSL-inactive)
5.  Pills stack right-to-left from the top, separated by `spacing` pixels
6.  active-glow-blur-radius > 0 → SDF glow halo radiates from all edges of active pill
7.  active-cutline-width > 0 → thin full-height line rendered flush to right window edge
      in the active tab's color (+ glow if active-glow-blur-radius > 0)
8.  User opens quick terminal → all pills appear inactive; cutline hidden
9.  User closes quick terminal → active pill highlight and cutline return
10. color_automation rule matches tab's program/path → pill uses override color
11. hide-if-single = true and only one tab → no pills or cutline drawn
```

### Visual Example

```
+------------------------------------------+--+--+
| $ cargo build                            |██|  |  ← active pill (tall, bright)
|    Compiling rio v0.2.0                  +--+  |  ← cutline (full height, active color)
|    ...                                   |█ |  |  ← tab 2 (short, dim)
|                                          +--+  |
|                                          |█ |  |  ← tab 3 (short, dim)
|                                          |  |  |
+------------------------------------------+--+--+
                                              ↑
                                        active-cutline-width
```

With hue rotation enabled, each pill and the cutline get the active tab's color
from an HSL sweep starting at `base-hue` with `hue-step` degrees between tabs.
With `active-glow-blur-radius > 0`, both the active pill and the cutline emit a
soft SDF glow matching the pill color.

## Architecture

### Rendering

The bookmark decorator is entirely contained in `ScreenNavigation::bookmark()`
at `frontends/rioterm/src/renderer/navigation.rs:98`.

Call chain:

```
Renderer::run()                              [renderer/mod.rs:1585]
  └─ ScreenNavigation::build_objects()      [navigation.rs:29]
       └─ bookmark()                         [navigation.rs:98]
            ├─ cutline glow Quad (optional)  (before pill loop)
            ├─ cutline Quad (optional)       (before pill loop)
            └─ per-tab loop:
                 ├─ active glow Quad (optional)
                 └─ pill Quad
```

`build_objects()` is called once per frame after terminal content rendering.
The resulting `Vec<Object>` is handed to `sugarloaf.set_objects()`.

### Cutline Construction (once, before pill loop)

When `active_cutline_width > 0` and `!qt_visible`:

1. **Color**: same `active_color` computed for the active pill (hue-rotation or
   `colors.tabs_active_highlight`)
2. **Glow layer** (if `active_glow_blur_radius > 0`): transparent-fill quad at
   `[win_w - line_w, 0.0]`, full window height, `shadow_offset = [0, 0]`,
   `shadow_blur_radius = active_glow_blur_radius` — produces an omnidirectional
   SDF bloom inward from the right edge
3. **Line quad**: solid `active_color` fill, `[win_w - line_w, 0.0]`, full
   window height, no border, no radius

### Quad Construction (per tab)

For each tab index `i` in `(0..len).rev()`:

1. **Active flag**: `is_active = !qt_visible && i == current`
2. **Color**:
   - If `hue_rotation`: compute `hsl_to_rgba(base_hue + i * hue_step, saturation, lightness_active/inactive, 1.0)`
   - Else if active: `colors.tabs_active_highlight`
   - Else: `colors.tabs`
   - Then: if `color_automation` has a rule matching `title.extra.program` /
     `title.extra.path`, override color with the rule's value
3. **Height**: `style.height_active` if active, else `style.height_inactive`
4. **Position**: `[initial_position, 0.0]` where `initial_position` starts at
   `(width / scale) - style.padding_x` and decrements by `style.spacing` each
   iteration (right-to-left stacking)
5. **Active edge glow** (optional, `active_glow_blur_radius > 0`): before the
   pill quad, push a transparent-fill quad at the same position/size with
   `shadow_offset = [0, 0]` and `shadow_blur_radius = active_glow_blur_radius`.
   This produces a symmetric SDF glow that bleeds outward from all four edges of
   the pill, creating a deep highlight halo. Glow color: `active_glow_color` if
   its alpha > 0, otherwise the pill color with alpha forced to 1.0.
6. **Pill quad fields**: fixed `width`, configurable `border_radius`,
   `border_width`, `border_color`, `shadow_color`, `shadow_offset`,
   `shadow_blur_radius`

### Config (`BookmarkStyle`)

Lives at `rio-backend/src/config/navigation.rs:97` as `pub struct BookmarkStyle`,
referenced in `Navigation` at line 390.

| TOML key | Rust field | Default (macOS / other) | Description |
|---|---|---|---|
| `width` | `width` | `15.0` | Pill width in logical px |
| `height-active` | `height_active` | `26.0` / `8.0` | Active pill height |
| `height-inactive` | `height_inactive` | `16.0` / `4.0` | Inactive pill height |
| `spacing` | `spacing` | `20.0` | Horizontal gap between pills |
| `padding-x` | `padding_x` | `30.0` | Right-edge offset from window border |
| `border-radius` | `border_radius` | `4.0` | Corner rounding (0 = sharp) |
| `border-width` | `border_width` | `0.0` | Border line width |
| `border-color` | `border_color` | `[0,0,0,0]` | Border color (RGBA) |
| `shadow-blur-radius` | `shadow_blur_radius` | `0.0` | Glow/shadow blur (0 = off) |
| `shadow-color` | `shadow_color` | `[0,0,0,0.4]` | Shadow color (RGBA) |
| `shadow-offset` | `shadow_offset` | `[0.0, 1.0]` | Shadow offset [x, y] |
| `active-glow-blur-radius` | `active_glow_blur_radius` | `0.0` | Blur radius of the active-tab edge glow halo (0 = off) |
| `active-glow-color` | `active_glow_color` | `[0,0,0,0]` | Glow halo color; transparent = use pill color |
| `active-cutline-width` | `active_cutline_width` | `0.0` | Width of full-height right-edge cutline in active tab color (0 = off) |
| `hue-rotation` | `hue_rotation` | `false` | Enable per-tab HSL color sweep |
| `base-hue` | `base_hue` | `0.0` | Starting hue in degrees (0–360) |
| `hue-step` | `hue_step` | `40.0` | Hue increment per tab (degrees) |
| `saturation` | `saturation` | `0.7` | HSL saturation (0.0–1.0) |
| `lightness-active` | `lightness_active` | `0.65` | HSL lightness for active tab |
| `lightness-inactive` | `lightness_inactive` | `0.35` | HSL lightness for inactive tabs |

`BookmarkStyle` derives `Debug, Clone, Copy, PartialEq, Serialize, Deserialize`
with `serde(rename_all = "kebab-case")` via per-field `rename` attributes.

### Color Sources (precedence, high → low)

```
color_automation rule (program + path match)
  └─► hue_rotation HSL color (if enabled)
        └─► colors.tabs_active_highlight (active, hue_rotation=false)
              └─► colors.tabs (inactive, hue_rotation=false)
```

### Quick Terminal Integration

`qt_visible` is passed from `Renderer::run()` via `build_objects()` to
`bookmark()`. When `true`, `is_active` is forced `false` for all tabs, so no
pill appears highlighted during quick terminal sessions. This prevents the
active-tab indicator from being misleading while the overlay owns focus.

### `hide_if_single`

`navigation.hide_if_single` (TOML: `hide-if-single`, default `true`) causes
`bookmark()` to return immediately when `len <= 1`, rendering no pills when
there is only a single tab.

## Key Files

| File | Lines | Role |
|---|---|---|
| `frontends/rioterm/src/renderer/navigation.rs` | 96–240 | `bookmark()` — full rendering implementation (pills + cutline) |
| `frontends/rioterm/src/renderer/navigation.rs` | 29–94 | `build_objects()` — dispatch to `bookmark()` |
| `rio-backend/src/config/navigation.rs` | 1–260 | `BookmarkStyle` struct and all default functions |
| `rio-backend/src/config/navigation.rs` | 400–425 | `Navigation` struct — `bookmark_style` field |
| `frontends/rioterm/src/renderer/mod.rs` | 1585–1593 | `build_objects()` call site in `Renderer::run()` |

## Example Config

```toml
[navigation]
mode = "Bookmark"
hide-if-single = true

[navigation.bookmark-style]
width = 15
height-active = 26
height-inactive = 16
spacing = 20
padding-x = 30
border-radius = 4
hue-rotation = true
base-hue = 180.0
hue-step = 40.0
saturation = 0.7
lightness-active = 0.65
lightness-inactive = 0.35
shadow-blur-radius = 6.0
shadow-color = "#000000"
shadow-offset = [0.0, 2.0]
# Active tab edge glow (deep highlight halo on pill)
active-glow-blur-radius = 10.0
# active-glow-color = "#ffa133"  # omit to use pill color
# Right-edge cutline in active tab color (mirrors window border glow)
active-cutline-width = 1.5
```
