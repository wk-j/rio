use crate::config::colors::deserialize_to_arr;
use crate::config::colors::ColorArray;
use serde::{Deserialize, Serialize};

/// Appearance and layout configuration for command overlay panels.
///
/// Command overlays are floating, click-through panels that run a real PTY
/// command and render its live terminal output on top of the terminal content.
///
/// TOML configuration example:
/// ```toml
/// [command-overlay]
/// x = 0.6
/// y = 0.05
/// width = 0.38
/// height = 0.55
/// opacity = 0.95
/// border-radius = 6.0
/// border-width = 1.0
/// border-color = '#44475a'
/// shadow-blur-radius = 8.0
/// shadow-color = '#00000066'
/// shadow-offset = [2.0, 4.0]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CommandOverlayStyle {
    /// Horizontal position as a fraction of window width (0.0–1.0).
    /// Default: 0.6 (right side of window).
    #[serde(default = "default_overlay_x")]
    pub x: f32,

    /// Vertical position as a fraction of window height (0.0–1.0).
    /// Default: 0.05 (near top of window).
    #[serde(default = "default_overlay_y")]
    pub y: f32,

    /// Width as a fraction of window width (0.0–1.0).
    /// Default: 0.38.
    #[serde(default = "default_overlay_width")]
    pub width: f32,

    /// Height as a fraction of window height (0.0–1.0).
    /// Default: 0.55.
    #[serde(default = "default_overlay_height")]
    pub height: f32,

    /// Opacity of the overlay panel (0.0 = fully transparent, 1.0 = opaque).
    /// Default: 1.0.
    #[serde(default = "default_overlay_opacity")]
    pub opacity: f32,

    /// Corner rounding radius in scaled pixels.
    /// Set to 0.0 for sharp corners. Default: 6.0.
    #[serde(default = "default_overlay_border_radius", rename = "border-radius")]
    pub border_radius: f32,

    /// Border width in scaled pixels (0.0 = no border).
    /// Default: 1.0.
    #[serde(default = "default_overlay_border_width", rename = "border-width")]
    pub border_width: f32,

    /// Border color as a hex string (e.g. '#44475a' or '#44475aFF').
    /// Default: transparent (uses terminal split color at render time).
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_overlay_border_color",
        rename = "border-color"
    )]
    pub border_color: ColorArray,

    /// Background color as a hex string. If transparent ([0,0,0,0]),
    /// the terminal's own background color is used.
    /// Default: transparent (inherits terminal background).
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_overlay_background_color",
        rename = "background-color"
    )]
    pub background_color: ColorArray,

    /// Shadow blur radius (0.0 = no shadow). Default: 0.0.
    #[serde(
        default = "default_overlay_shadow_blur_radius",
        rename = "shadow-blur-radius"
    )]
    pub shadow_blur_radius: f32,

    /// Shadow color as a hex string. Default: '#00000066'.
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_overlay_shadow_color",
        rename = "shadow-color"
    )]
    pub shadow_color: ColorArray,

    /// Shadow offset [x, y] in scaled pixels. Default: [0.0, 2.0].
    #[serde(default = "default_overlay_shadow_offset", rename = "shadow-offset")]
    pub shadow_offset: [f32; 2],
}

// --- Default value functions ---

#[inline]
fn default_overlay_x() -> f32 {
    0.6
}

#[inline]
fn default_overlay_y() -> f32 {
    0.05
}

#[inline]
fn default_overlay_width() -> f32 {
    0.38
}

#[inline]
fn default_overlay_height() -> f32 {
    0.55
}

#[inline]
fn default_overlay_opacity() -> f32 {
    1.0
}

#[inline]
fn default_overlay_border_radius() -> f32 {
    6.0
}

#[inline]
fn default_overlay_border_width() -> f32 {
    1.0
}

#[inline]
fn default_overlay_border_color() -> ColorArray {
    // Transparent — renderer will fall back to split/border color
    [0.0, 0.0, 0.0, 0.0]
}

#[inline]
fn default_overlay_background_color() -> ColorArray {
    // Transparent — renderer will fall back to terminal background
    [0.0, 0.0, 0.0, 0.0]
}

#[inline]
fn default_overlay_shadow_blur_radius() -> f32 {
    0.0
}

#[inline]
fn default_overlay_shadow_color() -> ColorArray {
    [0.0, 0.0, 0.0, 0.4]
}

#[inline]
fn default_overlay_shadow_offset() -> [f32; 2] {
    [0.0, 2.0]
}

impl Default for CommandOverlayStyle {
    fn default() -> Self {
        CommandOverlayStyle {
            x: default_overlay_x(),
            y: default_overlay_y(),
            width: default_overlay_width(),
            height: default_overlay_height(),
            opacity: default_overlay_opacity(),
            border_radius: default_overlay_border_radius(),
            border_width: default_overlay_border_width(),
            border_color: default_overlay_border_color(),
            background_color: default_overlay_background_color(),
            shadow_blur_radius: default_overlay_shadow_blur_radius(),
            shadow_color: default_overlay_shadow_color(),
            shadow_offset: default_overlay_shadow_offset(),
        }
    }
}

impl CommandOverlayStyle {
    /// Returns true if the background color is set to a non-transparent value
    /// (meaning the user explicitly configured a background color).
    #[inline]
    pub fn has_custom_background(&self) -> bool {
        self.background_color[3] > 0.0
    }

    /// Returns true if the border color is set to a non-transparent value
    /// (meaning the user explicitly configured a border color).
    #[inline]
    pub fn has_custom_border_color(&self) -> bool {
        self.border_color[3] > 0.0
    }
}
