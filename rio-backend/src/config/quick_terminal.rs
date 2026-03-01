use crate::config::colors::deserialize_to_arr;
use crate::config::colors::ColorArray;
use serde::{Deserialize, Serialize};

/// Configuration for the quick terminal overlay panel.
///
/// The quick terminal is a persistent floating panel anchored to the bottom
/// of the window, toggled with the `ToggleQuickTerminal` keybinding.
///
/// TOML configuration example:
/// ```toml
/// [quick-terminal]
/// height = 0.4
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
    /// Height of the panel as a fraction of window height (0.0–1.0).
    /// Default: 0.4 (40% of the window).
    #[serde(default = "default_qt_height")]
    pub height: f32,

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
            height: default_qt_height(),
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
