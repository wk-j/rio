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
/// background-color = '#1e1e2e'
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

    /// Background color as RGBA [r, g, b, a] (0.0–1.0 each).
    /// If fully transparent, the terminal's own background color is used.
    /// Default: transparent (inherits terminal background).
    #[serde(
        deserialize_with = "crate::config::colors::deserialize_to_arr",
        default = "default_qt_background_color",
        rename = "background-color"
    )]
    pub background_color: crate::config::colors::ColorArray,
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
fn default_qt_background_color() -> crate::config::colors::ColorArray {
    // Transparent — renderer will fall back to terminal background
    [0.0, 0.0, 0.0, 0.0]
}

impl Default for QuickTerminalConfig {
    fn default() -> Self {
        QuickTerminalConfig {
            position: QuickTerminalPosition::default(),
            height: default_qt_height(),
            width: default_qt_width(),
            opacity: default_qt_opacity(),
            background_color: default_qt_background_color(),
        }
    }
}

impl QuickTerminalConfig {
    /// Returns true if the background color is set to a non-transparent value.
    #[inline]
    pub fn has_custom_background(&self) -> bool {
        self.background_color[3] > 0.0
    }
}
