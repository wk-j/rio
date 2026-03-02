use crate::config::colors::deserialize_to_arr;
use crate::config::colors::ColorArray;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Position of the quick terminal panel within the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuickTerminalPosition {
    /// Panel anchored to the top edge (dropdown style, like Guake/Yakuake).
    Top,
    /// Panel anchored to the bottom edge (default).
    Bottom,
    /// Panel anchored to the left edge.
    Left,
    /// Panel anchored to the right edge.
    Right,
    /// Panel centered in the window.
    Center,
}

impl Default for QuickTerminalPosition {
    fn default() -> Self {
        QuickTerminalPosition::Bottom
    }
}

impl fmt::Display for QuickTerminalPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuickTerminalPosition::Top => write!(f, "top"),
            QuickTerminalPosition::Bottom => write!(f, "bottom"),
            QuickTerminalPosition::Left => write!(f, "left"),
            QuickTerminalPosition::Right => write!(f, "right"),
            QuickTerminalPosition::Center => write!(f, "center"),
        }
    }
}

impl QuickTerminalPosition {
    /// Returns true if the panel is oriented horizontally (top/bottom/center).
    /// These positions span the full window width (or use `width` for center)
    /// and use `height` for the vertical dimension.
    #[inline]
    pub fn is_horizontal(&self) -> bool {
        matches!(
            self,
            QuickTerminalPosition::Top
                | QuickTerminalPosition::Bottom
                | QuickTerminalPosition::Center
        )
    }

    /// Returns true if the panel is oriented vertically (left/right).
    /// These positions span the full window height and use `width`
    /// for the horizontal dimension.
    #[inline]
    pub fn is_vertical(&self) -> bool {
        matches!(
            self,
            QuickTerminalPosition::Left | QuickTerminalPosition::Right
        )
    }

    /// Returns the border-radius array [top-left, top-right, bottom-right, bottom-left]
    /// for this position. Corners that touch the window edge are set to 0.0.
    #[inline]
    pub fn border_radius(&self, r: f32) -> [f32; 4] {
        match self {
            // Bottom: rounded top corners, flat bottom
            QuickTerminalPosition::Bottom => [r, r, 0.0, 0.0],
            // Top: flat top, rounded bottom corners
            QuickTerminalPosition::Top => [0.0, 0.0, r, r],
            // Left: flat left corners, rounded right corners
            QuickTerminalPosition::Left => [0.0, r, r, 0.0],
            // Right: rounded left corners, flat right corners
            QuickTerminalPosition::Right => [r, 0.0, 0.0, r],
            // Center: all corners rounded
            QuickTerminalPosition::Center => [r, r, r, r],
        }
    }
}

/// Configuration for the quick terminal overlay panel.
///
/// The quick terminal is a persistent floating panel that can be anchored
/// to any edge of the window or centered, toggled with the
/// `ToggleQuickTerminal` keybinding.
///
/// TOML configuration example:
/// ```toml
/// [quick-terminal]
/// position = "bottom"
/// height = 0.4
/// width = 0.4
/// opacity = 1.0
/// border-radius = 6.0
/// border-width = 1.0
/// border-color = '#44475a'
/// shadow-blur-radius = 16.0
/// shadow-color = '#00000066'
/// shadow-offset = [0.0, -4.0]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuickTerminalConfig {
    /// Position of the panel within the window.
    /// Default: "bottom" (anchored to the bottom edge).
    #[serde(default)]
    pub position: QuickTerminalPosition,

    /// Panel height as a fraction of window height (0.0–1.0).
    /// Used by top, bottom, and center positions.
    /// Default: 0.4 (40% of the window height).
    #[serde(default = "default_qt_height")]
    pub height: f32,

    /// Panel width as a fraction of window width (0.0–1.0).
    /// Used by left, right, and center positions.
    /// For top/bottom the panel always spans the full window width.
    /// Default: 0.4 (40% of the window width).
    #[serde(default = "default_qt_width")]
    pub width: f32,

    /// Opacity of the panel background (0.0 = fully transparent, 1.0 = opaque).
    /// Default: 1.0.
    #[serde(default = "default_qt_opacity")]
    pub opacity: f32,

    /// Corner rounding radius in scaled pixels for the top corners.
    /// Set to 0.0 for sharp corners. Default: 6.0.
    #[serde(default = "default_qt_border_radius", rename = "border-radius")]
    pub border_radius: f32,

    /// Border width in scaled pixels (0.0 = no border). Default: 1.0.
    #[serde(default = "default_qt_border_width", rename = "border-width")]
    pub border_width: f32,

    /// Border color as a hex string (e.g. '#44475a').
    /// Default: transparent (uses terminal split color at render time).
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_qt_border_color",
        rename = "border-color"
    )]
    pub border_color: ColorArray,

    /// Background color as a hex string. If transparent ([0,0,0,0]),
    /// the terminal's own background color is used.
    /// Default: transparent (inherits terminal background).
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_qt_background_color",
        rename = "background-color"
    )]
    pub background_color: ColorArray,

    /// Shadow blur radius in scaled pixels (0.0 = no shadow). Default: 16.0.
    #[serde(
        default = "default_qt_shadow_blur_radius",
        rename = "shadow-blur-radius"
    )]
    pub shadow_blur_radius: f32,

    /// Shadow color as a hex string. Default: '#00000066'.
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_qt_shadow_color",
        rename = "shadow-color"
    )]
    pub shadow_color: ColorArray,

    /// Shadow offset [x, y] in scaled pixels. Default: [0.0, -4.0].
    #[serde(default = "default_qt_shadow_offset", rename = "shadow-offset")]
    pub shadow_offset: [f32; 2],
}

// --- Default value functions ---

#[inline]
fn default_qt_height() -> f32 {
    0.4
}

#[inline]
fn default_qt_width() -> f32 {
    0.4
}

#[inline]
fn default_qt_opacity() -> f32 {
    1.0
}

#[inline]
fn default_qt_border_radius() -> f32 {
    6.0
}

#[inline]
fn default_qt_border_width() -> f32 {
    1.0
}

#[inline]
fn default_qt_border_color() -> ColorArray {
    // Transparent — renderer will fall back to split/border color
    [0.0, 0.0, 0.0, 0.0]
}

#[inline]
fn default_qt_background_color() -> ColorArray {
    // Transparent — renderer will fall back to terminal background
    [0.0, 0.0, 0.0, 0.0]
}

#[inline]
fn default_qt_shadow_blur_radius() -> f32 {
    16.0
}

#[inline]
fn default_qt_shadow_color() -> ColorArray {
    [0.0, 0.0, 0.0, 0.4]
}

#[inline]
fn default_qt_shadow_offset() -> [f32; 2] {
    [0.0, -4.0]
}

impl Default for QuickTerminalConfig {
    fn default() -> Self {
        QuickTerminalConfig {
            position: QuickTerminalPosition::default(),
            height: default_qt_height(),
            width: default_qt_width(),
            opacity: default_qt_opacity(),
            border_radius: default_qt_border_radius(),
            border_width: default_qt_border_width(),
            border_color: default_qt_border_color(),
            background_color: default_qt_background_color(),
            shadow_blur_radius: default_qt_shadow_blur_radius(),
            shadow_color: default_qt_shadow_color(),
            shadow_offset: default_qt_shadow_offset(),
        }
    }
}

impl QuickTerminalConfig {
    /// Returns true if the background color is set to a non-transparent value.
    #[inline]
    pub fn has_custom_background(&self) -> bool {
        self.background_color[3] > 0.0
    }

    /// Returns true if the border color is set to a non-transparent value.
    #[inline]
    pub fn has_custom_border_color(&self) -> bool {
        self.border_color[3] > 0.0
    }
}
