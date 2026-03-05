mod char_cache;
mod font_cache;
mod leader;
pub mod navigation;
mod search;
pub mod utils;

use crate::context::renderable::TerminalSnapshot;
use crate::renderer::font_cache::FontCache;
use char_cache::CharCache;
use rio_backend::crosswords::LineDamage;
use rio_backend::event::TerminalDamage;

use crate::ansi::CursorShape;
use crate::context::renderable::{Cursor, RenderableContent};
use crate::context::ContextManager;
use crate::crosswords::grid::row::Row;
use crate::crosswords::pos::{Column, Line, Pos};
use crate::crosswords::square::{Flags, Square};
use navigation::ScreenNavigation;
use rio_backend::config::colors::term::TermColors;
use rio_backend::config::colors::{
    term::{List, DIM_FACTOR},
    AnsiColor, ColorArray, Colors, NamedColor,
};
use rio_backend::config::Config;
use rio_backend::config::CursorQuadDef;
use rio_backend::event::EventProxy;
use rio_backend::sugarloaf::{
    drawable_character, Content, CustomCursorQuad, FragmentStyle,
    FragmentStyleDecoration, Graphic, Quad, Stretch, Style, SugarCursor, Sugarloaf,
    UnderlineInfo, UnderlineShape, Weight,
};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ops::RangeInclusive;
use std::sync::LazyLock;

use unicode_width::UnicodeWidthChar;

#[derive(Default)]
pub struct Search {
    rich_text_id: Option<usize>,
    active_search: Option<String>,
}

#[derive(Default)]
pub struct LeaderMenu {
    rich_text_id: Option<usize>,
    active: bool,
    items: Vec<rio_backend::config::leader::LeaderItem>,
}

/// A recorded cursor position for the motion trail effect.
struct TrailEntry {
    /// Pixel position of the cursor [x, y].
    position: [f32; 2],
    /// Cell size at the time of recording [width, height].
    cell_size: [f32; 2],
    /// Cursor shape at the time of recording.
    cursor_shape: CursorShape,
    /// When the cursor arrived at this position.
    timestamp: std::time::Instant,
}

// --- Hex color preview helpers ---

static HEX_COLOR_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    // Longest alternatives first so the engine prefers 8/6-digit
    // over 3-digit. The 3-digit branch uses a word boundary (\b)
    // instead of a look-ahead (which the regex crate doesn't support)
    // to avoid matching inside longer hex strings.
    regex::Regex::new(
        r"(?:#[0-9a-fA-F]{8}\b|#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b|0x[0-9a-fA-F]{8}\b|0x[0-9a-fA-F]{6}\b)",
    )
    .unwrap()
});

struct DetectedColor {
    row: usize,
    col_start: usize,
    col_end: usize,
    color: ColorArray,
}

fn parse_hex_to_color(s: &str) -> Option<ColorArray> {
    let hex = if let Some(stripped) = s.strip_prefix("0x") {
        stripped
    } else if let Some(stripped) = s.strip_prefix('#') {
        stripped
    } else {
        return None;
    };

    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some([
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ])
        }
        _ => None,
    }
}

fn contrast_border_color(color: &ColorArray) -> ColorArray {
    let lum = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
    if lum > 0.5 {
        [0.2, 0.2, 0.2, 0.8]
    } else {
        [0.9, 0.9, 0.9, 0.5]
    }
}

fn detect_hex_colors(visible_rows: &[Row<Square>], columns: usize) -> Vec<DetectedColor> {
    let mut results = Vec::new();

    for (row_idx, row) in visible_rows.iter().enumerate() {
        let mut text = String::with_capacity(columns);
        let mut col_map: Vec<usize> = Vec::with_capacity(columns);

        for col in 0..columns.min(row.len()) {
            let square = &row.inner[col];
            if square.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            col_map.push(col);
            text.push(square.c);
        }

        for m in HEX_COLOR_RE.find_iter(&text) {
            let hex_str = m.as_str();
            if let Some(color) = parse_hex_to_color(hex_str) {
                let byte_start = m.start();
                let byte_end = m.end();
                if byte_start < col_map.len() && byte_end - 1 < col_map.len() {
                    let start_col = col_map[byte_start];
                    let end_col = col_map[byte_end - 1] + 1;
                    results.push(DetectedColor {
                        row: row_idx,
                        col_start: start_col,
                        col_end: end_col,
                        color,
                    });
                }
            }
        }
    }
    results
}

pub struct Renderer {
    is_vi_mode_enabled: bool,
    is_game_mode_enabled: bool,
    draw_bold_text_with_light_colors: bool,
    use_drawable_chars: bool,
    pub named_colors: Colors,
    pub colors: List,
    pub navigation: ScreenNavigation,
    unfocused_split_opacity: f32,
    last_active: Option<usize>,
    pub config_has_blinking_enabled: bool,
    pub config_blinking_interval: u64,
    ignore_selection_fg_color: bool,
    pub search: Search,
    pub leader_menu: LeaderMenu,
    #[allow(unused)]
    pub option_as_alt: String,
    #[allow(unused)]
    pub macos_use_unified_titlebar: bool,
    // Dynamic background keep track of the original bg color and
    // the same r,g,b with the mutated alpha channel.
    pub dynamic_background: ([f32; 4], wgpu::Color, bool),
    // Visual bell state
    visual_bell_active: bool,
    visual_bell_start: Option<std::time::Instant>,
    // Progress bar animation state
    progress_bar_anim_start: Option<std::time::Instant>,
    progress_bar_last_state: rio_backend::ansi::ProgressState,
    font_context: rio_backend::sugarloaf::font::FontLibrary,
    font_cache: FontCache,
    char_cache: CharCache,
    // Cursor glow config
    glow_config: rio_backend::config::CursorGlowConfig,
    // Custom cursor quad definitions (from config)
    cursor_quads: Vec<CursorQuadDef>,
    /// Resolved glow color as [r, g, b] (alpha applied per-layer).
    /// When glow color is "cursor", this is updated each frame from
    /// the cursor color to track theme/ANSI overrides.
    glow_resolved_color: Option<[f32; 3]>,
    // Cursor motion trail state
    /// Previous cursor pixel position for detecting movement.
    trail_last_pos: Option<[f32; 2]>,
    /// Trail entries recording recent cursor positions.
    trail_entries: VecDeque<TrailEntry>,
    /// Set to true during `run()` when trail entries are still
    /// fading out and the window needs continuous redraws.
    pub trail_animating: bool,
    // Window border glow config
    border_glow_config: rio_backend::config::window::BorderGlow,
    /// Monotonic start time for border glow animations.
    border_glow_start: std::time::Instant,
    /// Set to true during `run()` when border glow animation is
    /// active and the window needs continuous redraws.
    pub border_glow_animating: bool,
}

/// Resolve the glow color from config. Returns `None` when
/// color = "cursor" (resolved per-frame from the runtime cursor
/// color). Returns `Some([r, g, b])` for explicit hex colors.
fn resolve_glow_color(
    glow: &rio_backend::config::CursorGlowConfig,
    colors: &Colors,
) -> Option<[f32; 3]> {
    if glow.color == "cursor" {
        // Will be resolved per-frame from the cursor color
        None
    } else {
        let arr = rio_backend::config::colors::hex_to_color_arr(&glow.color);
        if arr[0] == 0.0 && arr[1] == 0.0 && arr[2] == 0.0 {
            // Fallback: use cursor color from theme
            Some([colors.cursor[0], colors.cursor[1], colors.cursor[2]])
        } else {
            Some([arr[0], arr[1], arr[2]])
        }
    }
}

/// Convert HSL (hue 0-360, saturation 0-1, lightness 0-1) to RGB [0-1].
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r1 + m, g1 + m, b1 + m]
}

/// Compute the window border glow overlay quads.
fn compute_border_glow(
    config: &rio_backend::config::window::BorderGlow,
    window_width: f32,
    window_height: f32,
    elapsed: f32,
) -> Vec<Quad> {
    use rio_backend::config::window::BorderGlowAnimate;

    if !config.enabled {
        return Vec::new();
    }

    let color_rgb = {
        let arr = rio_backend::config::colors::hex_to_color_arr(&config.color);
        [arr[0], arr[1], arr[2]]
    };

    let alpha = match config.animate {
        BorderGlowAnimate::None => config.glow_intensity,
        BorderGlowAnimate::Pulse => {
            let t = (elapsed * config.animate_speed * 2.0 * std::f32::consts::PI).sin();
            let base = config.glow_intensity;
            base * 0.5 + base * 0.5 * t
        }
        BorderGlowAnimate::Rainbow => config.glow_intensity,
    };

    let color = match config.animate {
        BorderGlowAnimate::Rainbow => {
            hsl_to_rgb((elapsed * config.animate_speed * 60.0) % 360.0, 0.8, 0.6)
        }
        _ => color_rgb,
    };

    let w = config.width;
    let blur = config.glow_radius;
    let shadow_color = [color[0], color[1], color[2], alpha];
    let fill_color = [color[0], color[1], color[2], alpha * 0.8];

    let make_edge = |pos: [f32; 2], size: [f32; 2]| -> Quad {
        Quad {
            position: pos,
            size,
            color: fill_color,
            border_radius: [0.0; 4],
            border_color: [0.0; 4],
            border_width: 0.0,
            shadow_color,
            shadow_offset: [0.0, 0.0],
            shadow_blur_radius: blur,
        }
    };

    // Chamfer: the diagonal cut at each corner. Edges stop short
    // and a staircase of tiny quads draws the 45-degree line.
    let chamfer = 15.0_f32.min(window_width * 0.03);

    let mut quads = Vec::with_capacity(20);

    // ── Straight edges (inset by chamfer at each end) ──
    quads.push(make_edge([chamfer, 0.0], [window_width - 2.0 * chamfer, w]));
    quads.push(make_edge(
        [chamfer, window_height - w],
        [window_width - 2.0 * chamfer, w],
    ));
    quads.push(make_edge(
        [0.0, chamfer],
        [w, window_height - 2.0 * chamfer],
    ));
    quads.push(make_edge(
        [window_width - w, chamfer],
        [w, window_height - 2.0 * chamfer],
    ));

    // ── Corner diagonals ──
    // Draw the 45-degree chamfer line as a staircase of small
    // square quads (w x w), each stepping one pixel diagonally.
    // These use the same glow as the edges for consistency.
    let steps = (chamfer / w).round().max(1.0) as usize;
    let sx = chamfer / steps as f32;
    let sy = chamfer / steps as f32;

    for i in 0..steps {
        let t = i as f32;

        // Top-left: from (chamfer, 0) going toward (0, chamfer)
        quads.push(make_edge([chamfer - (t + 1.0) * sx, t * sy], [sx, sy]));

        // Top-right: from (W - chamfer, 0) toward (W, chamfer)
        quads.push(make_edge(
            [window_width - chamfer + t * sx, t * sy],
            [sx, sy],
        ));

        // Bottom-left: from (chamfer, H) toward (0, H - chamfer)
        quads.push(make_edge(
            [chamfer - (t + 1.0) * sx, window_height - (t + 1.0) * sy],
            [sx, sy],
        ));

        // Bottom-right: from (W - chamfer, H) toward (W, H - chamfer)
        quads.push(make_edge(
            [
                window_width - chamfer + t * sx,
                window_height - (t + 1.0) * sy,
            ],
            [sx, sy],
        ));
    }

    quads
}

/// Compute border glow overlay quads for the quick terminal panel.
///
/// Renders a top+left L-shaped corner accent with a chamfered diagonal cut,
/// using the same `BorderGlow` config as the window border glow.
fn compute_quick_terminal_border_glow(
    config: &rio_backend::config::window::BorderGlow,
    panel_pos: [f32; 2],
    panel_size: [f32; 2],
    elapsed: f32,
) -> Vec<Quad> {
    use rio_backend::config::window::BorderGlowAnimate;

    if !config.enabled {
        return Vec::new();
    }

    // Resolve color: use quick_terminal_color if set, else fall back to color.
    // When quick_terminal_color is explicitly set, it pins the QT glow to that
    // fixed color and bypasses the rainbow animation (the window glow still animates).
    let qt_color_override = config.quick_terminal_color.as_deref();
    let color_str = qt_color_override.unwrap_or(&config.color);
    let color_rgb = {
        let arr = rio_backend::config::colors::hex_to_color_arr(color_str);
        [arr[0], arr[1], arr[2]]
    };

    let alpha = match config.animate {
        BorderGlowAnimate::None => config.glow_intensity,
        BorderGlowAnimate::Pulse => {
            let t = (elapsed * config.animate_speed * 2.0 * std::f32::consts::PI).sin();
            let base = config.glow_intensity;
            base * 0.5 + base * 0.5 * t
        }
        BorderGlowAnimate::Rainbow => config.glow_intensity,
    };

    // If quick_terminal_color is explicitly set, use it as a fixed color (no rainbow).
    // Otherwise follow the animation mode.
    let color = if qt_color_override.is_some() {
        color_rgb
    } else {
        match config.animate {
            BorderGlowAnimate::Rainbow => {
                hsl_to_rgb((elapsed * config.animate_speed * 60.0) % 360.0, 0.8, 0.6)
            }
            _ => color_rgb,
        }
    };

    let w = config.width;
    let blur = config.glow_radius;
    let shadow_color = [color[0], color[1], color[2], alpha];
    let fill_color = [color[0], color[1], color[2], alpha * 0.8];

    let px = panel_pos[0];
    let py = panel_pos[1];
    let pw = panel_size[0];
    let ph = panel_size[1];

    // Chamfer size matches compute_border_glow — panel-width relative, always visible.
    let chamfer = 15.0_f32.min(pw * 0.03);

    let make_edge = |pos: [f32; 2], size: [f32; 2]| -> Quad {
        Quad {
            position: pos,
            size,
            color: fill_color,
            border_radius: [0.0; 4],
            border_color: [0.0; 4],
            border_width: 0.0,
            shadow_color,
            shadow_offset: [0.0, 0.0],
            shadow_blur_radius: blur,
        }
    };

    // Only render the top edge + left edge + top-left chamfer corner.
    // This gives a consistent L-shaped corner accent regardless of QT position,
    // matching the style of the window border glow's top-left corner.
    let mut quads = Vec::with_capacity(12);

    // ── Top edge (from chamfer end to panel right) ──
    let top_seg_w = pw - chamfer;
    if top_seg_w > 0.0 {
        quads.push(make_edge([px + chamfer, py], [top_seg_w, w]));
    }

    // ── Left edge (from chamfer end to panel bottom) ──
    let left_seg_h = ph - chamfer;
    if left_seg_h > 0.0 {
        quads.push(make_edge([px, py + chamfer], [w, left_seg_h]));
    }

    // ── Top-left chamfer corner diagonal ──
    let steps = (chamfer / w).round().max(1.0) as usize;
    let sx = chamfer / steps as f32;
    let sy = chamfer / steps as f32;
    for i in 0..steps {
        let t = i as f32;
        quads.push(make_edge(
            [px + chamfer - (t + 1.0) * sx, py + t * sy],
            [sx, sy],
        ));
    }

    quads
}

impl Renderer {
    /// Build `CustomCursorQuad` objects from config-level
    /// `CursorQuadDef` entries. Color resolution is deferred to
    /// render time (cursor_color), but per-quad overrides with
    /// explicit hex colors are resolved here.
    fn build_custom_cursor_quads(&self) -> Vec<CustomCursorQuad> {
        let fallback = self.named_colors.cursor;
        self.cursor_quads
            .iter()
            .map(|def| {
                let base_color = def
                    .color
                    .as_ref()
                    .map(|hex| rio_backend::config::colors::hex_to_color_arr(hex))
                    .unwrap_or(fallback);
                let color = [
                    base_color[0],
                    base_color[1],
                    base_color[2],
                    base_color[3] * def.opacity,
                ];
                CustomCursorQuad {
                    rel_rect: [def.x, def.y, def.width, def.height],
                    color,
                    border_radius: def.border_radius,
                    border_width: def.border_width,
                }
            })
            .collect()
    }

    pub fn new(
        config: &Config,
        font_context: &rio_backend::sugarloaf::font::FontLibrary,
    ) -> Renderer {
        let colors = List::from(&config.colors);
        let named_colors = config.colors;

        let mut dynamic_background =
            (named_colors.background.0, named_colors.background.1, false);
        if config.window.opacity < 1. {
            dynamic_background.1.a = config.window.opacity as f64;
            dynamic_background.2 = true;
        } else if config.window.background_image.is_some() {
            dynamic_background.1 = wgpu::Color::TRANSPARENT;
            dynamic_background.2 = true;
        }

        let mut color_automation: HashMap<String, HashMap<String, [f32; 4]>> =
            HashMap::new();

        for rule in &config.navigation.color_automation {
            color_automation
                .entry(rule.program.clone())
                .or_default()
                .insert(rule.path.clone(), rule.color);
        }

        let mut renderer = Renderer {
            unfocused_split_opacity: config.navigation.unfocused_split_opacity,
            last_active: None,
            use_drawable_chars: config.fonts.use_drawable_chars,
            draw_bold_text_with_light_colors: config.draw_bold_text_with_light_colors,
            macos_use_unified_titlebar: config.window.macos_use_unified_titlebar,
            config_blinking_interval: config.cursor.blinking_interval.clamp(350, 1200),
            option_as_alt: config.option_as_alt.to_lowercase(),
            is_vi_mode_enabled: false,
            config_has_blinking_enabled: config.cursor.blinking,
            ignore_selection_fg_color: config.ignore_selection_fg_color,
            colors,
            navigation: ScreenNavigation::new(
                config.navigation.clone(),
                color_automation,
                config.padding_y,
            ),
            named_colors,
            dynamic_background,
            visual_bell_active: false,
            visual_bell_start: None,
            progress_bar_anim_start: None,
            progress_bar_last_state: rio_backend::ansi::ProgressState::Hidden,
            search: Search::default(),
            leader_menu: LeaderMenu::default(),
            font_cache: FontCache::new(),
            font_context: font_context.clone(),
            char_cache: CharCache::new(),
            is_game_mode_enabled: config.renderer.strategy.is_game(),
            glow_config: config.cursor.glow.clone(),
            cursor_quads: config.cursor.quads.clone(),
            glow_resolved_color: resolve_glow_color(&config.cursor.glow, &config.colors),
            trail_last_pos: None,
            trail_entries: VecDeque::new(),
            trail_animating: false,
            border_glow_config: config.window.border_glow.clone(),
            border_glow_start: std::time::Instant::now(),
            border_glow_animating: false,
        };

        // Pre-populate font cache with common characters for better performance
        renderer.font_cache.pre_populate(font_context);

        renderer
    }

    #[inline]
    pub fn set_active_search(&mut self, active_search: Option<String>) {
        self.search.active_search = active_search;
    }

    #[inline]
    pub fn set_leader_menu(
        &mut self,
        active: bool,
        items: Vec<rio_backend::config::leader::LeaderItem>,
    ) {
        self.leader_menu.active = active;
        self.leader_menu.items = items;
    }

    #[inline]
    fn create_style(
        &mut self,
        square: &Square,
        term_colors: &TermColors,
    ) -> (FragmentStyle, char) {
        let flags = square.flags;

        let mut foreground_color = self.compute_color(&square.fg, flags, term_colors);
        let mut background_color = self.compute_bg_color(square, term_colors);

        let content = if square.c == '\t' || flags.contains(Flags::HIDDEN) {
            ' '
        } else {
            square.c
        };

        let font_attrs = match (
            flags.contains(Flags::ITALIC),
            flags.contains(Flags::BOLD_ITALIC),
            flags.contains(Flags::BOLD),
        ) {
            (true, _, _) => (Stretch::NORMAL, Weight::NORMAL, Style::Italic),
            (_, true, _) => (Stretch::NORMAL, Weight::BOLD, Style::Italic),
            (_, _, true) => (Stretch::NORMAL, Weight::BOLD, Style::Normal),
            _ => (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
        };

        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut background_color, &mut foreground_color);
        }

        let background_color = if self.dynamic_background.2
            && background_color[0] == self.dynamic_background.0[0]
            && background_color[1] == self.dynamic_background.0[1]
            && background_color[2] == self.dynamic_background.0[2]
        {
            None
        } else {
            Some(background_color)
        };

        let (decoration, decoration_color) = self.compute_decoration(square, term_colors);

        (
            FragmentStyle {
                color: foreground_color,
                background_color,
                font_attrs: font_attrs.into(),
                decoration,
                decoration_color,
                ..FragmentStyle::default()
            },
            content,
        )
    }

    #[inline]
    fn compute_decoration(
        &self,
        square: &Square,
        term_colors: &TermColors,
    ) -> (Option<FragmentStyleDecoration>, Option<[f32; 4]>) {
        let mut decoration = None;
        let mut decoration_color = None;

        if square.flags.contains(Flags::UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Regular,
            }));
        } else if square.flags.contains(Flags::STRIKEOUT) {
            decoration = Some(FragmentStyleDecoration::Strikethrough);
        } else if square.flags.contains(Flags::DOUBLE_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                is_doubled: true,
                shape: UnderlineShape::Regular,
            }));
        } else if square.flags.contains(Flags::DOTTED_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Dotted,
            }));
        } else if square.flags.contains(Flags::DASHED_UNDERLINE) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Dashed,
            }));
        } else if square.flags.contains(Flags::UNDERCURL) {
            decoration = Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                is_doubled: false,
                shape: UnderlineShape::Curly,
            }));
        }

        if decoration.is_some() {
            if let Some(color) = square.underline_color() {
                decoration_color =
                    Some(self.compute_color(&color, square.flags, term_colors));
            }
        };

        (decoration, decoration_color)
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    /// Check if a position is within any hint match
    fn is_position_in_hint_matches(
        matches: &[rio_backend::crosswords::search::Match],
        pos: Pos,
    ) -> bool {
        matches.iter().any(|m| m.contains(&pos))
    }

    #[allow(clippy::too_many_arguments)]
    fn create_line(
        &mut self,
        builder: &mut Content,
        row: &Row<Square>,
        has_cursor: bool,
        line_opt: Option<usize>,
        line: Line,
        renderable_content: &RenderableContent,
        hint_matches: Option<&[rio_backend::crosswords::search::Match]>,
        focused_match: &Option<RangeInclusive<Pos>>,
        term_colors: &TermColors,
        is_active: bool,
        bg_opacity_override: Option<f32>,
    ) {
        // let start = std::time::Instant::now();
        let cursor = &renderable_content.cursor;
        let selection_range = renderable_content.selection_range;
        let columns: usize = row.len();
        let mut content = String::with_capacity(columns);
        let mut last_char_was_space = false;
        let mut last_style = FragmentStyle::default();

        // Collect all characters that need font lookups to batch them
        let mut font_lookups = Vec::new();
        let mut styles_and_chars = Vec::with_capacity(columns);

        // First pass: collect all styles and identify font cache misses
        for column in 0..columns {
            let square = &row.inner[column];

            if square.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            let (mut style, mut square_content) =
                if has_cursor && column == cursor.state.pos.col {
                    self.create_cursor_style(square, cursor, is_active, term_colors)
                } else {
                    self.create_style(square, term_colors)
                };

            // Apply underline for hyperlinks (OSC 8) or highlighted hints (hover)
            let should_underline = square.hyperlink().is_some() || {
                if let Some(highlighted_hint) = &renderable_content.highlighted_hint {
                    let current_pos = Pos::new(line, Column(column));
                    highlighted_hint.start <= current_pos
                        && current_pos <= highlighted_hint.end
                } else {
                    false
                }
            };

            if should_underline {
                style.decoration =
                    Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                        is_doubled: false,
                        shape: UnderlineShape::Regular,
                    }));
            }

            // Check selection more efficiently
            if let Some(ref range) = selection_range {
                let pos = Pos::new(line, Column(column));
                if range.contains(pos) {
                    style.color = if self.ignore_selection_fg_color {
                        self.compute_color(&square.fg, square.flags, term_colors)
                    } else {
                        self.named_colors.selection_foreground
                    };
                    style.background_color = Some(self.named_colors.selection_background);
                }
            } else if let Some(matches) = hint_matches {
                let pos = Pos::new(line, Column(column));
                if Self::is_position_in_hint_matches(matches, pos) {
                    let is_focused =
                        focused_match.as_ref().is_some_and(|fm| fm.contains(&pos));
                    if is_focused {
                        style.color = self.named_colors.search_focused_match_foreground;
                        style.background_color =
                            Some(self.named_colors.search_focused_match_background);
                    } else {
                        style.color = self.named_colors.search_match_foreground;
                        style.background_color =
                            Some(self.named_colors.search_match_background);
                    }
                }
            }

            // Check for hint labels at this position
            if let Some(hint_label) = self.find_hint_label_at_position(
                renderable_content,
                Pos::new(line, Column(column)),
            ) {
                // Override character with hint label character if available
                if let Some(label_char) = hint_label.label.first() {
                    square_content = *label_char;
                }

                // Apply hint label styling
                if hint_label.is_first {
                    // Use configurable hint colors
                    style.color = self.named_colors.hint_foreground;
                    style.background_color = Some(self.named_colors.hint_background);
                } else {
                    // End colors: use same foreground, slightly dimmed background
                    style.color = self.named_colors.hint_foreground;
                    let mut dimmed_bg = self.named_colors.hint_background;
                    // Dim the background slightly for continuation labels
                    dimmed_bg[0] *= 0.8;
                    dimmed_bg[1] *= 0.8;
                    dimmed_bg[2] *= 0.8;
                    style.background_color = Some(dimmed_bg);
                }

                // Make hint labels bold for better visibility
                use rio_backend::sugarloaf::font_introspector::{Attributes, Weight};
                let current_attrs = style.font_attrs;
                style.font_attrs = Attributes::new(
                    current_attrs.stretch(),
                    Weight::BOLD,
                    current_attrs.style(),
                );
            }

            if !is_active {
                style.color[3] = self.unfocused_split_opacity;
                if let Some(ref mut background_color) = style.background_color {
                    background_color[3] = self.unfocused_split_opacity;
                }
            }

            // For overlay surfaces (e.g. quick terminal, command overlays),
            // ensure cells have proper backgrounds. Cells with explicit
            // ANSI colors (set by the program) keep their full opacity.
            // Cells with the default background get the overlay's opacity
            // so only the panel background is translucent.
            //
            // When dynamic_background.2 is false (no window transparency /
            // background image), create_style always emits Some(bg_color)
            // even for default-background cells, so the is_none() branch
            // would never fire. We also handle the Some(color) case: if the
            // cell's background matches the terminal's named background color
            // it is a "default" cell and should receive the opacity override.
            if let Some(opacity) = bg_opacity_override {
                let named_bg = self.named_colors.background.0;
                match style.background_color {
                    None => {
                        let mut bg = named_bg;
                        bg[3] = opacity;
                        style.background_color = Some(bg);
                    }
                    Some(ref mut bg)
                        if bg[0] == named_bg[0]
                            && bg[1] == named_bg[1]
                            && bg[2] == named_bg[2] =>
                    {
                        bg[3] = opacity;
                    }
                    _ => {}
                }
            }

            if square.flags.contains(Flags::GRAPHICS) {
                let graphic = &square.graphics().unwrap()[0];
                style.media = Some(Graphic {
                    id: graphic.texture.id,
                    offset_x: graphic.offset_x,
                    offset_y: graphic.offset_y,
                });
                style.background_color = None;
            }

            // Handle drawable characters
            if self.use_drawable_chars {
                if let Some(character) = drawable_character(square_content) {
                    style.drawable_char = Some(character);
                }
            }

            let has_drawable_char = style.drawable_char.is_some();
            if !has_drawable_char {
                if let Some((font_id, width)) =
                    self.font_cache.get(&(square_content, style.font_attrs))
                {
                    style.font_id = *font_id;
                    style.width = *width;
                } else {
                    // Mark this character for font lookup
                    font_lookups.push((
                        styles_and_chars.len(),
                        square_content,
                        style.font_attrs,
                    ));
                }
            }

            styles_and_chars.push((style, square_content, column));
        }

        // Batch font lookups with a single lock acquisition
        if !font_lookups.is_empty() {
            let font_ctx = self.font_context.inner.read();
            for (style_index, square_content, font_attrs) in font_lookups {
                let mut width = square_content.width().unwrap_or(1) as f32;
                let style = &mut styles_and_chars[style_index].0;

                if let Some((font_id, is_emoji)) =
                    font_ctx.find_best_font_match(square_content, style)
                {
                    style.font_id = font_id;
                    if is_emoji {
                        width = 2.0;
                    }
                }
                style.width = width;

                self.font_cache
                    .insert((square_content, font_attrs), (style.font_id, style.width));
            }
        }

        // Second pass: render the line using the resolved styles
        for (style, square_content, column) in styles_and_chars {
            // Handle drawable characters
            if style.drawable_char.is_some() {
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_text_on_line(line, &content, last_style);
                    } else {
                        builder.add_text(&content, last_style);
                    }
                    content.clear();
                }

                last_style = style;
                content.push(' '); // Ignore font shaping
            } else {
                if square_content == ' ' {
                    if !last_char_was_space {
                        if !content.is_empty() {
                            if let Some(line) = line_opt {
                                builder.add_text_on_line(line, &content, last_style);
                            } else {
                                builder.add_text(&content, last_style);
                            }
                            content.clear();
                        }

                        last_char_was_space = true;
                        last_style = style;
                    }
                } else {
                    if last_char_was_space && !content.is_empty() {
                        if let Some(line) = line_opt {
                            builder.add_text_on_line(line, &content, last_style);
                        } else {
                            builder.add_text(&content, last_style);
                        }
                        content.clear();
                    }

                    last_char_was_space = false;
                }

                if last_style != style {
                    if !content.is_empty() {
                        if let Some(line) = line_opt {
                            builder.add_text_on_line(line, &content, last_style);
                        } else {
                            builder.add_text(&content, last_style);
                        }
                        content.clear();
                    }

                    last_style = style;
                }

                content.push(square_content);
            }

            // Render last column and break row
            if column == (columns - 1) {
                if !content.is_empty() {
                    if let Some(line) = line_opt {
                        builder.add_text_on_line(line, &content, last_style);
                    } else {
                        builder.add_text(&content, last_style);
                    }
                }

                break;
            }
        }

        if let Some(line) = line_opt {
            builder.build_line(line);
        } else {
            builder.new_line();
        }

        // let duration = start.elapsed();
        // println!(
        //     "Time elapsed in --renderer.update.create_line() is: {:?}",
        //     duration
        // );
    }

    #[inline]
    fn compute_color(
        &self,
        color: &AnsiColor,
        flags: Flags,
        term_colors: &TermColors,
    ) -> ColorArray {
        match color {
            AnsiColor::Named(ansi) => {
                match (
                    self.draw_bold_text_with_light_colors,
                    flags & Flags::DIM_BOLD,
                ) {
                    // If no bright foreground is set, treat it like the BOLD flag doesn't exist.
                    (_, Flags::DIM_BOLD)
                        if ansi == &NamedColor::Foreground
                            && self.named_colors.light_foreground.is_none() =>
                    {
                        self.color(NamedColor::DimForeground as usize, term_colors)
                    }
                    // Draw bold text in bright colors *and* contains bold flag.
                    (true, Flags::BOLD) => {
                        self.color(ansi.to_light() as usize, term_colors)
                    }
                    // Cell is marked as dim and not bold.
                    (_, Flags::DIM) | (false, Flags::DIM_BOLD) => {
                        self.color(ansi.to_dim() as usize, term_colors)
                    }
                    // None of the above, keep original color..
                    _ => self.color(*ansi as usize, term_colors),
                }
            }
            AnsiColor::Spec(rgb) => {
                if !flags.contains(Flags::DIM) {
                    rgb.to_arr()
                } else {
                    rgb.to_arr_with_dim()
                }
            }
            AnsiColor::Indexed(index) => {
                let index = match (flags & Flags::DIM_BOLD, index) {
                    (Flags::DIM, 8..=15) => *index as usize - 8,
                    (Flags::DIM, 0..=7) => {
                        NamedColor::DimBlack as usize + *index as usize
                    }
                    _ => *index as usize,
                };

                self.color(index, term_colors)
            }
        }
    }

    #[inline]
    fn compute_bg_color(&self, square: &Square, term_colors: &TermColors) -> ColorArray {
        match square.bg {
            AnsiColor::Named(ansi) => self.color(ansi as usize, term_colors),
            AnsiColor::Spec(rgb) => match square.flags & Flags::DIM {
                Flags::DIM => (&(rgb * DIM_FACTOR)).into(),
                _ => (&rgb).into(),
            },
            AnsiColor::Indexed(idx) => {
                let idx = match (
                    self.draw_bold_text_with_light_colors,
                    square.flags & Flags::DIM_BOLD,
                    idx,
                ) {
                    (true, Flags::BOLD, 0..=7) => idx as usize + 8,
                    (false, Flags::DIM, 8..=15) => idx as usize - 8,
                    (false, Flags::DIM, 0..=7) => {
                        NamedColor::DimBlack as usize + idx as usize
                    }
                    _ => idx as usize,
                };

                self.color(idx, term_colors)
            }
        }
    }

    #[inline]
    fn create_cursor_style(
        &self,
        square: &Square,
        cursor: &Cursor,
        is_active: bool,
        term_colors: &TermColors,
    ) -> (FragmentStyle, char) {
        let font_attrs = match (
            square.flags.contains(Flags::ITALIC),
            square.flags.contains(Flags::BOLD_ITALIC),
            square.flags.contains(Flags::BOLD),
        ) {
            (true, _, _) => (Stretch::NORMAL, Weight::NORMAL, Style::Italic),
            (_, true, _) => (Stretch::NORMAL, Weight::BOLD, Style::Italic),
            (_, _, true) => (Stretch::NORMAL, Weight::BOLD, Style::Normal),
            _ => (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
        };

        let mut color = self.compute_color(&square.fg, square.flags, term_colors);
        let mut background_color = self.compute_bg_color(square, term_colors);
        // If IME is enabled we get the current content to cursor
        let content = if cursor.is_ime_enabled {
            cursor.content
        } else {
            square.c
        };

        if square.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut background_color, &mut color);
        }

        let has_dynamic_background = self.dynamic_background.2
            && background_color[0] == self.dynamic_background.0[0]
            && background_color[1] == self.dynamic_background.0[1]
            && background_color[2] == self.dynamic_background.0[2];
        let is_block_like = matches!(
            cursor.state.content,
            CursorShape::Block | CursorShape::Custom
        );
        let background_color = if has_dynamic_background && (!is_block_like && is_active)
        {
            None
        } else {
            Some(background_color)
        };

        // If IME is or cursor is block enabled, put background color
        // when cursor is over the character
        match (cursor.is_ime_enabled, (is_block_like || !is_active)) {
            (_, true) => {
                color = self.named_colors.background.0;
            }
            (true, false) => {
                color = self.named_colors.foreground;
            }
            (false, false) => {}
        }

        let mut style = FragmentStyle {
            color,
            background_color,
            font_attrs: font_attrs.into(),
            ..FragmentStyle::default()
        };

        let cursor_color = if !self.is_vi_mode_enabled {
            term_colors[NamedColor::Cursor].unwrap_or(self.named_colors.cursor)
        } else {
            self.named_colors.vi_cursor
        };

        let (decoration, decoration_color) = self.compute_decoration(square, term_colors);
        style.decoration = decoration;
        style.decoration_color = decoration_color;

        match cursor.state.content {
            CursorShape::Underline => {
                style.decoration =
                    Some(FragmentStyleDecoration::Underline(UnderlineInfo {
                        is_doubled: false,
                        shape: UnderlineShape::Regular,
                    }));
                style.decoration_color = Some(cursor_color);
            }
            CursorShape::Block => {
                style.cursor = Some(SugarCursor::Block(cursor_color));
            }
            CursorShape::Beam => {
                style.cursor = Some(SugarCursor::Caret(cursor_color));
            }
            CursorShape::Hidden => {}
            CursorShape::Custom => {
                if self.cursor_quads.is_empty() {
                    // Fallback to block if no quads defined
                    style.cursor = Some(SugarCursor::Block(cursor_color));
                } else {
                    style.cursor = Some(SugarCursor::Custom(cursor_color));
                }
            }
        }

        if !is_active {
            style.decoration = None;
            style.cursor = Some(SugarCursor::HollowBlock(cursor_color));
        }

        (style, content)
    }

    #[inline]
    pub fn set_vi_mode(&mut self, is_vi_mode_enabled: bool) {
        self.is_vi_mode_enabled = is_vi_mode_enabled;
    }

    /// Trigger the visual bell
    #[inline]
    pub fn trigger_visual_bell(&mut self) {
        self.visual_bell_active = true;
        self.visual_bell_start = Some(std::time::Instant::now());
    }

    /// Check if visual bell should be displayed and update its state
    #[inline]
    pub fn update_visual_bell(&mut self) -> bool {
        if !self.visual_bell_active {
            return false;
        }

        if let Some(start_time) = self.visual_bell_start {
            if start_time.elapsed() >= crate::constants::BELL_DURATION {
                self.visual_bell_active = false;
                self.visual_bell_start = None;
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    // Get the RGB value for a color index.
    #[inline]
    pub fn color(&self, color: usize, term_colors: &TermColors) -> ColorArray {
        term_colors[color].unwrap_or(self.colors[color])
    }

    #[inline]
    fn update_search_rich_text(&mut self, content: &mut Content) {
        if let Some(active_search_content) = &self.search.active_search {
            if let Some(search_rich_text) = self.search.rich_text_id {
                if active_search_content.is_empty() {
                    content
                        .sel(search_rich_text)
                        .clear()
                        .new_line()
                        .add_text(
                            &String::from("Search: type something..."),
                            FragmentStyle {
                                color: [
                                    self.named_colors.foreground[0],
                                    self.named_colors.foreground[1],
                                    self.named_colors.foreground[2],
                                    self.named_colors.foreground[3] - 0.3,
                                ],
                                ..FragmentStyle::default()
                            },
                        )
                        .build();
                } else {
                    let style = FragmentStyle {
                        color: self.named_colors.foreground,
                        ..FragmentStyle::default()
                    };
                    let line = content.sel(search_rich_text);
                    line.clear().new_line().add_text("Search: ", style);

                    // Collect characters that need font lookups
                    let mut font_lookups = Vec::new();
                    let mut char_styles = Vec::new();

                    for character in active_search_content.chars() {
                        let mut char_style = style;
                        if let Some((font_id, width)) =
                            self.font_cache.get(&(character, style.font_attrs))
                        {
                            char_style.font_id = *font_id;
                            char_style.width = *width;
                        } else {
                            font_lookups.push((char_styles.len(), character));
                        }
                        char_styles.push((char_style, character));
                    }

                    // Batch font lookups with a single lock acquisition
                    if !font_lookups.is_empty() {
                        let font_ctx = self.font_context.inner.read();
                        for (style_index, character) in font_lookups {
                            let mut width = character.width().unwrap_or(1) as f32;
                            let char_style = &mut char_styles[style_index].0;

                            if let Some((font_id, is_emoji)) =
                                font_ctx.find_best_font_match(character, char_style)
                            {
                                char_style.font_id = font_id;
                                if is_emoji {
                                    width = 2.0;
                                }
                            }
                            char_style.width = width;
                        }
                    }

                    // Render all characters
                    for (char_style, character) in char_styles {
                        line.add_text_on_line(
                            // Add on first line
                            1,
                            self.char_cache.get_str(character),
                            char_style,
                        );
                    }

                    line.build();
                }
            }
        }
    }

    fn update_leader_rich_text(&self, content: &mut Content, rich_text_id: usize) {
        let title_style = FragmentStyle {
            color: self.named_colors.foreground,
            ..FragmentStyle::default()
        };

        let key_style = FragmentStyle {
            color: [0.54, 0.71, 0.99, 1.0], // Blue highlight for keys
            ..FragmentStyle::default()
        };

        let label_style = FragmentStyle {
            color: self.named_colors.foreground,
            ..FragmentStyle::default()
        };

        let line = content.sel(rich_text_id);
        line.clear();
        line.new_line();
        line.add_text("Rio Commands", title_style);
        line.new_line();
        line.new_line();

        for item in &self.leader_menu.items {
            let key_display = match item.key {
                ' ' => "SPC".to_string(),
                '\n' => "RET".to_string(),
                '\t' => "TAB".to_string(),
                c => format!(" {} ", c),
            };

            line.add_text(&key_display, key_style);
            line.add_text("  ", label_style);
            line.add_text(&item.label, label_style);
            line.new_line();
        }

        line.build();
    }

    #[inline]
    pub fn run(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        context_manager: &mut ContextManager<EventProxy>,
        focused_match: &Option<RangeInclusive<Pos>>,
    ) -> Option<crate::context::renderable::WindowUpdate> {
        // let start = std::time::Instant::now();

        // Set custom cursor quad definitions for this frame
        if !self.cursor_quads.is_empty() {
            let quads = self.build_custom_cursor_quads();
            sugarloaf.set_custom_cursor_quads(quads);
        } else {
            sugarloaf.set_custom_cursor_quads(Vec::new());
        }

        // In case rich text for search was not created
        let has_search = self.search.active_search.is_some();
        if has_search && self.search.rich_text_id.is_none() {
            let search_rich_text = sugarloaf.create_temp_rich_text();
            sugarloaf.set_rich_text_font_size(&search_rich_text, 12.0);
            self.search.rich_text_id = Some(search_rich_text);
        }

        let grid = context_manager.current_grid_mut();
        let active_key = grid.current;
        let zoomed_key = grid.zoomed_key;
        let qt_visible = grid.is_quick_terminal_visible();
        let mut has_active_changed = false;
        if self.last_active != Some(active_key) {
            has_active_changed = true;
            self.last_active = Some(active_key);
        }

        for (key, grid_context) in grid.contexts_mut().iter_mut() {
            // When zoomed, skip rendering all splits except the zoomed one
            if let Some(zk) = zoomed_key {
                if *key != zk {
                    continue;
                }
            }

            // When the quick terminal is visible, clear main pane rich
            // text so its cell backgrounds and glyphs don't render on
            // top of the QT content.
            if qt_visible {
                let ctx = grid_context.context_mut();
                let rt_id = ctx.rich_text_id;
                let content = sugarloaf.content();
                content.sel(rt_id);
                content.clear();
                content.build();
                ctx.renderable_content.pending_update.reset();
                continue;
            }

            let is_active = &active_key == key;
            let context = grid_context.context_mut();

            let mut has_ime = false;
            if let Some(preedit) = context.ime.preedit() {
                if let Some(content) = preedit.text.chars().next() {
                    context.renderable_content.cursor.content = content;
                    context.renderable_content.cursor.is_ime_enabled = true;
                    has_ime = true;
                }
            }

            if !has_ime {
                context.renderable_content.cursor.is_ime_enabled = false;
                context.renderable_content.cursor.content =
                    context.renderable_content.cursor.content_ref;
            }

            let force_full_damage = has_active_changed || self.is_game_mode_enabled;

            // Check if we need to render
            if !context.renderable_content.pending_update.is_dirty() && !force_full_damage
            {
                // No updates pending, skip rendering
                continue;
            }

            // Get UI damage before resetting
            let ui_damage = context.renderable_content.pending_update.take_ui_damage();
            context.renderable_content.pending_update.reset();

            // Compute snapshot at render time
            let terminal_snapshot = {
                let mut terminal = context.terminal.lock();

                // Get damage from terminal
                let terminal_damage = if force_full_damage {
                    Some(TerminalDamage::Full)
                } else {
                    terminal.peek_damage_event()
                };

                // Merge terminal damage with UI damage
                let damage = match (terminal_damage, ui_damage) {
                    (Some(TerminalDamage::Full), _) | (_, Some(TerminalDamage::Full)) => {
                        TerminalDamage::Full
                    }
                    (Some(term), Some(ui)) => {
                        // Merge partial damages
                        match (term, ui) {
                            (
                                TerminalDamage::Partial(mut lines1),
                                TerminalDamage::Partial(lines2),
                            ) => {
                                lines1.extend(lines2);
                                TerminalDamage::Partial(lines1)
                            }
                            _ => TerminalDamage::Full,
                        }
                    }
                    (Some(damage), None) => damage,
                    (None, Some(damage)) => damage,
                    (None, None) => TerminalDamage::Full,
                };

                let snapshot = TerminalSnapshot {
                    colors: terminal.colors,
                    display_offset: terminal.display_offset(),
                    blinking_cursor: terminal.blinking_cursor,
                    visible_rows: terminal.visible_rows(),
                    cursor: terminal.cursor(),
                    damage,
                    columns: terminal.columns(),
                    screen_lines: terminal.screen_lines(),
                    progress_state: terminal.progress_state,
                };
                terminal.reset_damage();
                drop(terminal);

                snapshot
            };

            // Get hint matches from renderable content
            let hint_matches = context.renderable_content.hint_matches.as_deref();

            // Update cursor state from snapshot
            context.renderable_content.cursor.state = terminal_snapshot.cursor;

            let mut specific_lines: Option<BTreeSet<LineDamage>> = None;

            // Check for partial damage to optimize rendering
            if !force_full_damage {
                match terminal_snapshot.damage {
                    TerminalDamage::Full => {
                        // Full damage, render everything
                    }
                    TerminalDamage::Partial(lines) => {
                        if !lines.is_empty() {
                            specific_lines = Some(lines.clone());
                        }
                    }
                    TerminalDamage::CursorOnly => {
                        specific_lines = Some(
                            [LineDamage {
                                line: *context.renderable_content.cursor.state.pos.row
                                    as usize,
                                damaged: true,
                            }]
                            .into_iter()
                            .collect(),
                        );
                    }
                }
            }

            let rich_text_id = context.rich_text_id;

            let mut is_cursor_visible =
                context.renderable_content.cursor.state.is_visible();
            context.renderable_content.has_blinking_enabled =
                terminal_snapshot.blinking_cursor;

            if terminal_snapshot.blinking_cursor {
                let has_selection = context.renderable_content.selection_range.is_some();
                if !has_selection {
                    let mut should_blink = true;
                    if let Some(last_typing_time) = context.renderable_content.last_typing
                    {
                        if last_typing_time.elapsed() < std::time::Duration::from_secs(1)
                        {
                            should_blink = false;
                        }
                    }

                    if should_blink {
                        let now = std::time::Instant::now();
                        let should_toggle = if let Some(last_blink) =
                            context.renderable_content.last_blink_toggle
                        {
                            now.duration_since(last_blink).as_millis()
                                >= self.config_blinking_interval as u128
                        } else {
                            // First time: start with cursor visible and set initial timing
                            context.renderable_content.is_blinking_cursor_visible = true;
                            context.renderable_content.last_blink_toggle = Some(now);
                            false // Don't toggle on first frame
                        };

                        if should_toggle {
                            context.renderable_content.is_blinking_cursor_visible =
                                !context.renderable_content.is_blinking_cursor_visible;
                            context.renderable_content.last_blink_toggle = Some(now);
                        }
                    } else {
                        // When not blinking (e.g., during typing), ensure cursor is visible
                        context.renderable_content.is_blinking_cursor_visible = true;
                        // Reset blink timing when not blinking so it starts fresh when blinking resumes
                        context.renderable_content.last_blink_toggle = None;
                    }
                } else {
                    // When there's a selection, keep cursor visible and reset blink timing
                    context.renderable_content.is_blinking_cursor_visible = true;
                    context.renderable_content.last_blink_toggle = None;
                }

                is_cursor_visible = context.renderable_content.is_blinking_cursor_visible;
            }

            if !is_active && context.renderable_content.cursor.state.is_visible() {
                is_cursor_visible = true;
            }

            let content = sugarloaf.content();
            match specific_lines {
                None => {
                    content.sel(rich_text_id);
                    content.clear();
                    for (i, row) in terminal_snapshot.visible_rows.iter().enumerate() {
                        let has_cursor = is_cursor_visible
                            && context.renderable_content.cursor.state.pos.row == i;
                        self.create_line(
                            content,
                            row,
                            has_cursor,
                            None,
                            Line((i as i32) - terminal_snapshot.display_offset as i32),
                            &context.renderable_content,
                            hint_matches,
                            focused_match,
                            &terminal_snapshot.colors,
                            is_active,
                            None,
                        );
                    }
                    content.build();
                    // let _duration = start.elapsed();
                }
                Some(lines) => {
                    content.sel(rich_text_id);
                    for line in lines {
                        let line = line.line;
                        let has_cursor = is_cursor_visible
                            && context.renderable_content.cursor.state.pos.row == line;
                        content.clear_line(line);
                        if let Some(visible_row) =
                            terminal_snapshot.visible_rows.get(line)
                        {
                            self.create_line(
                                content,
                                visible_row,
                                has_cursor,
                                Some(line),
                                Line(
                                    (line as i32)
                                        - terminal_snapshot.display_offset as i32,
                                ),
                                &context.renderable_content,
                                hint_matches,
                                focused_match,
                                &terminal_snapshot.colors,
                                is_active,
                                None,
                            );
                        }
                    }

                    // let _duration = start.elapsed();
                }
            }
        }

        // Render quick terminal content if visible.
        // Cell backgrounds use the panel opacity so they fully cover
        // main-pane cell backgrounds rendered earlier in the same pass
        // (all quads render before all rich texts in sugarloaf).
        let qt_opacity = grid.quick_terminal_config.opacity;
        if let Some(ref mut qt) = grid.quick_terminal {
            if qt.visible {
                let is_active = qt.item.val.route_id == active_key;
                let context = qt.item.context_mut();

                let mut has_ime = false;
                if let Some(preedit) = context.ime.preedit() {
                    if let Some(content) = preedit.text.chars().next() {
                        context.renderable_content.cursor.content = content;
                        context.renderable_content.cursor.is_ime_enabled = true;
                        has_ime = true;
                    }
                }
                if !has_ime {
                    context.renderable_content.cursor.is_ime_enabled = false;
                    context.renderable_content.cursor.content =
                        context.renderable_content.cursor.content_ref;
                }

                context.renderable_content.pending_update.reset();

                let terminal_snapshot = {
                    let mut terminal = context.terminal.lock();
                    let snapshot = TerminalSnapshot {
                        colors: terminal.colors,
                        display_offset: terminal.display_offset(),
                        blinking_cursor: terminal.blinking_cursor,
                        visible_rows: terminal.visible_rows(),
                        cursor: terminal.cursor(),
                        damage: TerminalDamage::Full,
                        columns: terminal.columns(),
                        screen_lines: terminal.screen_lines(),
                        progress_state: terminal.progress_state,
                    };
                    terminal.reset_damage();
                    drop(terminal);
                    snapshot
                };

                context.renderable_content.cursor.state = terminal_snapshot.cursor;

                let rich_text_id = context.rich_text_id;
                let is_cursor_visible =
                    context.renderable_content.cursor.state.is_visible();

                let content = sugarloaf.content();
                content.sel(rich_text_id);
                content.clear();
                for (i, row) in terminal_snapshot.visible_rows.iter().enumerate() {
                    let has_cursor = is_cursor_visible
                        && context.renderable_content.cursor.state.pos.row == i;
                    self.create_line(
                        content,
                        row,
                        has_cursor,
                        None,
                        Line((i as i32) - terminal_snapshot.display_offset as i32),
                        &context.renderable_content,
                        None,  // no hint matches for quick terminal
                        &None, // no focused match
                        &terminal_snapshot.colors,
                        is_active,
                        Some(qt_opacity),
                    );
                }
                content.build();
            }
        }

        // Render command overlay terminal content (live PTY output)
        for overlay in grid.command_overlays.iter_mut() {
            if !overlay.visible {
                continue;
            }
            let context = overlay.item.context_mut();
            context.renderable_content.pending_update.reset();

            let terminal_snapshot = {
                let mut terminal = context.terminal.lock();
                let snapshot = TerminalSnapshot {
                    colors: terminal.colors,
                    display_offset: terminal.display_offset(),
                    blinking_cursor: terminal.blinking_cursor,
                    visible_rows: terminal.visible_rows(),
                    cursor: terminal.cursor(),
                    damage: TerminalDamage::Full,
                    columns: terminal.columns(),
                    screen_lines: terminal.screen_lines(),
                    progress_state: terminal.progress_state,
                };
                terminal.reset_damage();
                drop(terminal);
                snapshot
            };

            context.renderable_content.cursor.state = terminal_snapshot.cursor;

            let rich_text_id = context.rich_text_id;
            let is_cursor_visible = context.renderable_content.cursor.state.is_visible();

            let content = sugarloaf.content();
            content.sel(rich_text_id);
            content.clear();
            for (i, row) in terminal_snapshot.visible_rows.iter().enumerate() {
                let has_cursor = is_cursor_visible
                    && context.renderable_content.cursor.state.pos.row == i;
                self.create_line(
                    content,
                    row,
                    has_cursor,
                    None,
                    Line((i as i32) - terminal_snapshot.display_offset as i32),
                    &context.renderable_content,
                    None,  // no hint matches for command overlays
                    &None, // no focused match
                    &terminal_snapshot.colors,
                    true, // always render as "active"
                    Some(grid.command_overlay_style.opacity),
                );
            }
            content.build();
        }

        self.update_search_rich_text(sugarloaf.content());

        let window_size = sugarloaf.window_size();
        let scale_factor = sugarloaf.scale_factor();
        let mut objects = Vec::with_capacity(15);
        self.navigation.build_objects(
            sugarloaf,
            (window_size.width, window_size.height, scale_factor),
            &self.named_colors,
            context_manager,
            self.search.active_search.is_some(),
            &mut objects,
            qt_visible,
        );

        if has_search {
            if let Some(rich_text_id) = self.search.rich_text_id {
                search::draw_search_bar(
                    &mut objects,
                    rich_text_id,
                    &self.named_colors,
                    (window_size.width, window_size.height, scale_factor),
                );
            }

            self.search.active_search = None;
            self.search.rich_text_id = None;
        }

        // Leader menu overlay
        if self.leader_menu.active {
            // Create rich text for leader menu if needed (use persistent, not temp)
            if self.leader_menu.rich_text_id.is_none() {
                let leader_rich_text = sugarloaf.create_rich_text();
                let terminal_font_size = sugarloaf.style().font_size;
                sugarloaf.set_rich_text_font_size(&leader_rich_text, terminal_font_size);
                self.leader_menu.rich_text_id = Some(leader_rich_text);
            }

            if let Some(rich_text_id) = self.leader_menu.rich_text_id {
                // Update rich text content with proper styling
                self.update_leader_rich_text(sugarloaf.content(), rich_text_id);

                // Get actual cell height from the rich text layout so
                // the background quad matches the rendered content.
                let layout = sugarloaf.rich_text_layout(&rich_text_id);
                let cell_height = layout.dimensions.height / layout.dimensions.scale;

                leader::draw_leader_menu(
                    &mut objects,
                    rich_text_id,
                    &self.named_colors,
                    &self.leader_menu.items,
                    (window_size.width, window_size.height, scale_factor),
                    cell_height,
                );
            }
        }

        // let _duration = start.elapsed();
        context_manager
            .extend_with_grid_objects(&mut objects, self.named_colors.background.0);
        // let _duration = start.elapsed();

        // Update visual bell state and set overlay if needed
        let visual_bell_active = self.update_visual_bell();

        // Set visual bell overlay that renders on top of everything
        let bell_overlay = if visual_bell_active {
            Some(Quad {
                position: [0.0, 0.0],
                size: [window_size.width, window_size.height],
                color: self.named_colors.foreground,
                ..Quad::default()
            })
        } else {
            None
        };
        sugarloaf.set_visual_bell_overlay(bell_overlay);

        // Set vi mode background tint overlay
        let vi_mode_overlay = if self.is_vi_mode_enabled {
            Some(Quad {
                position: [0.0, 0.0],
                size: [window_size.width, window_size.height],
                color: self.named_colors.vi_mode_background,
                ..Quad::default()
            })
        } else {
            None
        };
        sugarloaf.set_vi_mode_overlay(vi_mode_overlay);

        // Color preview: scan visible terminal content for hex
        // color codes and render colored quads covering each one.
        {
            let grid = context_manager.current_grid();
            if grid.color_preview_active {
                let mut color_quads: Vec<Quad> = Vec::new();

                for grid_context in grid.contexts_ordered() {
                    let pane_pos = grid_context.position();
                    let dim = &grid_context.val.dimension;
                    let scale = dim.dimension.scale;
                    let cell_w = dim.dimension.width / scale;
                    let cell_h = (dim.dimension.height / scale) * dim.line_height;
                    let columns = dim.columns;

                    let visible_rows = {
                        let terminal = grid_context.val.terminal.lock();
                        terminal.visible_rows()
                    };

                    let detected = detect_hex_colors(&visible_rows, columns);

                    for dc in &detected {
                        let quad_x = pane_pos[0] + (dc.col_start as f32) * cell_w;
                        let quad_y = pane_pos[1] + (dc.row as f32) * cell_h;
                        let char_count = (dc.col_end - dc.col_start) as f32;
                        let quad_w = char_count * cell_w;
                        let quad_h = cell_h;
                        let border = contrast_border_color(&dc.color);

                        color_quads.push(Quad {
                            position: [quad_x, quad_y],
                            size: [quad_w, quad_h],
                            color: dc.color,
                            border_radius: [2.0; 4],
                            border_color: border,
                            border_width: 1.0,
                            shadow_color: [0.0; 4],
                            shadow_offset: [0.0; 2],
                            shadow_blur_radius: 0.0,
                        });
                    }
                }

                sugarloaf.set_color_preview_overlay(color_quads);
            } else {
                sugarloaf.set_color_preview_overlay(Vec::new());
            }
        }

        // Build cursor glow layers: concentric quads that create
        // a bloom effect behind the cursor cell. Shape adapts to
        // cursor type (block/beam/underline), color derived from
        // the cursor/theme color.
        //
        // When trail is enabled, fading ghost quads are rendered
        // along the cursor's recent movement path.
        let cursor_glow_layers = {
            let glow = &self.glow_config;
            if !glow.enabled {
                self.trail_animating = false;
                vec![]
            } else {
                let grid = context_manager.current_grid();
                let ctx = grid.current();
                let cursor = &ctx.renderable_content.cursor;

                if !cursor.state.is_visible() {
                    self.trail_animating = false;
                    vec![]
                } else {
                    let pane_pos = grid.current_position();
                    let dim = &ctx.dimension;
                    let scale = dim.dimension.scale;

                    let cell_w = dim.dimension.width / scale;
                    let cell_h = (dim.dimension.height / scale) * dim.line_height;

                    let col = *cursor.state.pos.col;
                    let row = *cursor.state.pos.row as usize;

                    let cursor_x = pane_pos[0] + (col as f32) * cell_w;
                    let cursor_y = pane_pos[1] + (row as f32) * cell_h;

                    // Resolve glow color: from config or cursor color
                    let glow_rgb = self.glow_resolved_color.unwrap_or([
                        self.named_colors.cursor[0],
                        self.named_colors.cursor[1],
                        self.named_colors.cursor[2],
                    ]);

                    let cursor_shape = cursor.state.content;

                    // Shape-aware base size: adapt to cursor shape
                    let (base_w, base_h) = match cursor_shape {
                        CursorShape::Block | CursorShape::Custom => (cell_w, cell_h),
                        CursorShape::Beam => (2.0, cell_h),
                        CursorShape::Underline => (cell_w, 2.0),
                        CursorShape::Hidden => (cell_w, cell_h),
                    };

                    // --- Trail: detect movement and record ---
                    let cur_pos = [cursor_x, cursor_y];
                    if glow.trail {
                        let moved = match self.trail_last_pos {
                            Some(prev) => {
                                (prev[0] - cur_pos[0]).abs() > 0.5
                                    || (prev[1] - cur_pos[1]).abs() > 0.5
                            }
                            None => false,
                        };
                        if moved {
                            // Record the OLD position as a trail
                            // ghost before updating
                            if let Some(prev) = self.trail_last_pos {
                                self.trail_entries.push_back(TrailEntry {
                                    position: prev,
                                    cell_size: [cell_w, cell_h],
                                    cursor_shape,
                                    timestamp: std::time::Instant::now(),
                                });
                                // Cap entries to avoid unbounded
                                // growth
                                let max = glow.trail_segments.clamp(2, 12) as usize * 2;
                                while self.trail_entries.len() > max {
                                    self.trail_entries.pop_front();
                                }
                            }
                        }
                        self.trail_last_pos = Some(cur_pos);
                    } else {
                        self.trail_last_pos = None;
                        self.trail_entries.clear();
                    }

                    // --- Trail: evict expired entries ---
                    let trail_dur = std::time::Duration::from_secs_f32(
                        glow.trail_duration.clamp(0.05, 2.0),
                    );
                    let now = std::time::Instant::now();
                    while let Some(front) = self.trail_entries.front() {
                        if now.duration_since(front.timestamp) >= trail_dur {
                            self.trail_entries.pop_front();
                        } else {
                            break;
                        }
                    }

                    // --- Trail: generate ghost quads ---
                    let intensity = glow.intensity.clamp(0.01, 1.0);
                    let radius = glow.radius.clamp(0.5, 5.0);
                    let layers = glow.layers.clamp(1, 5) as usize;

                    let trail_count = self.trail_entries.len();
                    let mut quads = Vec::with_capacity(trail_count + layers);

                    for entry in self.trail_entries.iter() {
                        let age = now.duration_since(entry.timestamp).as_secs_f32();
                        let trail_secs = glow.trail_duration.clamp(0.05, 2.0);
                        let fade = 1.0 - (age / trail_secs);
                        if fade <= 0.0 {
                            continue;
                        }

                        // Shape-aware size for the trail ghost
                        let (tw, th) = match entry.cursor_shape {
                            CursorShape::Block | CursorShape::Custom => {
                                (entry.cell_size[0], entry.cell_size[1])
                            }
                            CursorShape::Beam => (2.0, entry.cell_size[1]),
                            CursorShape::Underline => (entry.cell_size[0], 2.0),
                            CursorShape::Hidden => {
                                (entry.cell_size[0], entry.cell_size[1])
                            }
                        };

                        // Single glow layer per trail ghost,
                        // with padding proportional to the
                        // outermost glow layer
                        let pad = entry.cell_size[0] * radius * 0.6;
                        let alpha = intensity * fade * 0.4;

                        let gw = tw + pad * 2.0;
                        let gh = th + pad * 2.0;
                        let gx =
                            entry.position[0] - pad + (entry.cell_size[0] - tw) / 2.0;
                        let gy =
                            entry.position[1] - pad + (entry.cell_size[1] - th) / 2.0;

                        let br = match entry.cursor_shape {
                            CursorShape::Underline => gh / 2.0,
                            CursorShape::Block
                            | CursorShape::Beam
                            | CursorShape::Hidden
                            | CursorShape::Custom => gw.min(gh) / 2.0,
                        };

                        quads.push(Quad {
                            position: [gx, gy],
                            size: [gw, gh],
                            color: [glow_rgb[0], glow_rgb[1], glow_rgb[2], alpha],
                            border_radius: [br; 4],
                            ..Quad::default()
                        });
                    }

                    self.trail_animating = !self.trail_entries.is_empty();

                    // --- Glow: concentric bloom layers ---
                    for i in (0..layers).rev() {
                        // Outer layers are larger with lower alpha
                        let t = (i as f32 + 1.0) / layers as f32;
                        let pad = cell_w * radius * t;
                        let alpha = intensity * (1.0 - t) * 0.8 + intensity * 0.2;

                        let glow_w = base_w + pad * 2.0;
                        let glow_h = base_h + pad * 2.0;
                        let glow_x = cursor_x - pad + (cell_w - base_w) / 2.0;
                        let glow_y = cursor_y - pad + (cell_h - base_h) / 2.0;

                        // Adaptive border radius: fully round for
                        // block/beam, flatter for underline
                        let br = match cursor_shape {
                            CursorShape::Underline => glow_h / 2.0,
                            CursorShape::Block
                            | CursorShape::Beam
                            | CursorShape::Hidden
                            | CursorShape::Custom => glow_w.min(glow_h) / 2.0,
                        };

                        quads.push(Quad {
                            position: [glow_x, glow_y],
                            size: [glow_w, glow_h],
                            color: [glow_rgb[0], glow_rgb[1], glow_rgb[2], alpha],
                            border_radius: [br; 4],
                            ..Quad::default()
                        });
                    }
                    quads
                }
            }
        };
        sugarloaf.set_cursor_glow_layers(cursor_glow_layers);

        // Compute window border glow overlay
        {
            let elapsed = self.border_glow_start.elapsed().as_secs_f32();
            let border_glow_quads = compute_border_glow(
                &self.border_glow_config,
                window_size.width,
                window_size.height,
                elapsed,
            );
            self.border_glow_animating = self.border_glow_config.enabled
                && self.border_glow_config.animate
                    != rio_backend::config::window::BorderGlowAnimate::None;
            sugarloaf.set_window_border_glow(border_glow_quads);
        }

        // Compute quick terminal border glow overlay (same style as window
        // border glow, but positioned around the QT panel bounds).
        {
            let elapsed = self.border_glow_start.elapsed().as_secs_f32();
            let qt_glow_quads = if self.border_glow_config.enabled {
                if let Some((pw, ph, pos)) = context_manager
                    .current_grid()
                    .quick_terminal_glow_geometry()
                {
                    compute_quick_terminal_border_glow(
                        &self.border_glow_config,
                        pos,
                        [pw, ph],
                        elapsed,
                    )
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            sugarloaf.set_quick_terminal_border_glow(qt_glow_quads);
        }

        // Set progress bar from active terminal's progress state
        let progress_bar = {
            use rio_backend::ansi::ProgressState;
            let current_context = context_manager.current_grid_mut().current_mut();
            let terminal = current_context.terminal.lock();
            let progress_state = terminal.progress_state;
            drop(terminal);

            // Detect state transitions to start animation
            if progress_state != self.progress_bar_last_state {
                let should_animate =
                    match (&self.progress_bar_last_state, &progress_state) {
                        // Animate when transitioning to a visible result state
                        (_, ProgressState::Success { .. })
                        | (_, ProgressState::Error { .. })
                        | (_, ProgressState::Normal { .. })
                        | (_, ProgressState::Warning { .. }) => true,
                        _ => false,
                    };
                if should_animate {
                    self.progress_bar_anim_start = Some(std::time::Instant::now());
                } else {
                    self.progress_bar_anim_start = None;
                }
                self.progress_bar_last_state = progress_state;
            }

            if progress_state.is_visible() {
                const PROGRESS_BAR_HEIGHT: f32 = 3.0;
                // Animation duration in seconds
                const ANIM_DURATION: f32 = 2.0;

                let progress_ratio = match progress_state {
                    ProgressState::Normal { progress } => progress as f32 / 100.0,
                    ProgressState::Error { progress } => progress as f32 / 100.0,
                    ProgressState::Warning { progress } => progress as f32 / 100.0,
                    ProgressState::Success { progress } => progress as f32 / 100.0,
                    ProgressState::Indeterminate => {
                        // For indeterminate, show a pulsing segment
                        // Use time-based animation
                        let time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f32();
                        let cycle = (time * 1.0) % 2.0;
                        if cycle < 1.0 {
                            cycle
                        } else {
                            2.0 - cycle
                        }
                    }
                    ProgressState::Hidden => 0.0,
                };

                // Apply fill animation: grow from 0 to target ratio over ANIM_DURATION
                let (animated_ratio, animation_running) =
                    if let Some(start) = self.progress_bar_anim_start {
                        let elapsed = start.elapsed().as_secs_f32();
                        if elapsed < ANIM_DURATION {
                            // Ease-out cubic: fast start, smooth deceleration
                            let t = elapsed / ANIM_DURATION;
                            let eased = 1.0 - (1.0 - t).powi(3);
                            (progress_ratio * eased, true)
                        } else {
                            (progress_ratio, false)
                        }
                    } else {
                        (progress_ratio, false)
                    };

                // If animation is still running, keep rendering
                if animation_running {
                    let ctx = context_manager.current_grid_mut().current_mut();
                    ctx.renderable_content.pending_update.set_dirty();
                }

                let color = match progress_state {
                    ProgressState::Normal { .. } | ProgressState::Indeterminate => {
                        [0.2, 0.6, 1.0, 1.0] // Blue
                    }
                    ProgressState::Error { .. } => [1.0, 0.3, 0.3, 1.0], // Red
                    ProgressState::Warning { .. } => [1.0, 0.8, 0.2, 1.0], // Yellow
                    ProgressState::Success { .. } => [0.3, 0.8, 0.4, 1.0], // Green
                    ProgressState::Hidden => [0.0, 0.0, 0.0, 0.0],
                };

                let (x, width) = if matches!(progress_state, ProgressState::Indeterminate)
                {
                    // For indeterminate, show a moving segment
                    let segment_width = window_size.width * 0.3;
                    let x = animated_ratio * (window_size.width - segment_width);
                    (x, segment_width)
                } else {
                    (0.0, window_size.width * animated_ratio)
                };

                Some(Quad {
                    position: [x, 0.0],
                    size: [width, PROGRESS_BAR_HEIGHT],
                    color,
                    ..Quad::default()
                })
            } else {
                None
            }
        };
        sugarloaf.set_progress_bar(progress_bar);

        sugarloaf.set_objects(objects);
        // Apply background color from current context if changed
        let current_context = context_manager.current_grid_mut().current_mut();
        let window_update = if let Some(bg_state) =
            current_context.renderable_content.background.take()
        {
            use crate::context::renderable::BackgroundState;
            match bg_state {
                BackgroundState::Set(color) => {
                    sugarloaf.set_background_color(Some(color));
                }
                BackgroundState::Reset => {
                    sugarloaf.set_background_color(None);
                }
            }
            Some(crate::context::renderable::WindowUpdate::Background(
                bg_state,
            ))
        } else {
            None
        };

        sugarloaf.render();

        // let _duration = start.elapsed();
        window_update
    }

    /// Find hint label at the specified position
    fn find_hint_label_at_position<'a>(
        &self,
        renderable_content: &'a RenderableContent,
        pos: Pos,
    ) -> Option<&'a crate::context::renderable::HintLabel> {
        renderable_content
            .hint_labels
            .iter()
            .find(|label| label.position == pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::crosswords::pos::{Column, Line, Pos};

    #[test]
    fn test_is_position_in_hint_matches() {
        let matches = vec![
            Pos::new(Line(0), Column(0))..=Pos::new(Line(0), Column(4)),
            Pos::new(Line(1), Column(5))..=Pos::new(Line(1), Column(9)),
            Pos::new(Line(5), Column(10))..=Pos::new(Line(5), Column(15)),
        ];

        // Test positions inside matches
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(0))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(2))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(4))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(5))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(7))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(9))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(10))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(15))
        ));

        // Test positions outside matches
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(5))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(4))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(1), Column(10))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(0))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(9))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(5), Column(16))
        ));
    }

    #[test]
    fn test_empty_hint_matches() {
        let matches: Vec<rio_backend::crosswords::search::Match> = vec![];

        // Any position should return false for empty matches
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(0), Column(0))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(10), Column(20))
        ));
    }

    #[test]
    fn test_single_character_match() {
        let matches = vec![Pos::new(Line(3), Column(7))..=Pos::new(Line(3), Column(7))];

        // Test the exact position
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(3), Column(7))
        ));

        // Test adjacent positions
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(3), Column(6))
        ));
        assert!(!Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(3), Column(8))
        ));
    }

    #[test]
    fn test_overlapping_matches() {
        // In practice, matches shouldn't overlap, but let's test the behavior
        let matches = vec![
            Pos::new(Line(2), Column(5))..=Pos::new(Line(2), Column(10)),
            Pos::new(Line(2), Column(8))..=Pos::new(Line(2), Column(12)),
        ];

        // Test positions in the overlap
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(8))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(9))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(10))
        ));

        // Test positions in non-overlapping parts
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(5))
        ));
        assert!(Renderer::is_position_in_hint_matches(
            &matches,
            Pos::new(Line(2), Column(12))
        ));
    }
}
