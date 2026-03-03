use crate::config::defaults::*;
use serde::{Deserialize, Serialize};
use sugarloaf::ImageProperties;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BorderGlowAnimate {
    #[default]
    None,
    Pulse,
    Rainbow,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BorderGlow {
    #[serde(default)]
    pub enabled: bool,
    /// Color for the window border glow.
    #[serde(default = "default_border_glow_color")]
    pub color: String,
    /// Override color for the quick terminal border accent.
    /// Defaults to `color` when not set.
    #[serde(default)]
    pub quick_terminal_color: Option<String>,
    #[serde(default = "default_border_glow_width")]
    pub width: f32,
    #[serde(default = "default_border_glow_radius")]
    pub glow_radius: f32,
    #[serde(default = "default_border_glow_intensity")]
    pub glow_intensity: f32,
    #[serde(default)]
    pub animate: BorderGlowAnimate,
    #[serde(default = "default_border_glow_animate_speed")]
    pub animate_speed: f32,
}

fn default_border_glow_color() -> String {
    String::from("#8B5CF6")
}

fn default_border_glow_width() -> f32 {
    2.0
}

fn default_border_glow_radius() -> f32 {
    8.0
}

fn default_border_glow_intensity() -> f32 {
    0.6
}

fn default_border_glow_animate_speed() -> f32 {
    1.0
}

impl Default for BorderGlow {
    fn default() -> Self {
        Self {
            enabled: false,
            color: default_border_glow_color(),
            quick_terminal_color: None,
            width: default_border_glow_width(),
            glow_radius: default_border_glow_radius(),
            glow_intensity: default_border_glow_intensity(),
            animate: BorderGlowAnimate::default(),
            animate_speed: default_border_glow_animate_speed(),
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum WindowMode {
    #[serde(alias = "maximized")]
    Maximized,
    #[serde(alias = "fullscreen")]
    Fullscreen,
    // Windowed will use width and height definition
    #[default]
    #[serde(alias = "windowed")]
    Windowed,
}

#[derive(Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum Colorspace {
    #[serde(alias = "srgb")]
    Srgb,
    #[serde(alias = "display-p3")]
    DisplayP3,
    #[serde(alias = "rec2020")]
    Rec2020,
}

#[cfg(target_os = "macos")]
#[allow(clippy::derivable_impls)]
impl Default for Colorspace {
    fn default() -> Colorspace {
        Colorspace::DisplayP3
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::derivable_impls)]
impl Default for Colorspace {
    fn default() -> Colorspace {
        Colorspace::Srgb
    }
}

#[derive(Clone, Serialize, Deserialize, Copy, Debug, PartialEq)]
pub enum Decorations {
    #[serde(alias = "enabled")]
    Enabled,
    #[serde(alias = "disabled")]
    Disabled,
    #[serde(alias = "transparent")]
    Transparent,
    #[serde(alias = "buttonless")]
    Buttonless,
}

#[allow(clippy::derivable_impls)]
impl Default for Decorations {
    fn default() -> Decorations {
        Decorations::Enabled
    }
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub enum WindowsCornerPreference {
    #[serde(alias = "default")]
    Default = 0,
    #[serde(alias = "donotround")]
    DoNotRound = 1,
    #[serde(alias = "round")]
    Round = 2,
    #[serde(alias = "roundsmall")]
    RoundSmall = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlignMode {
    /// CR-001: focused left, others stacked right (default)
    Side,
    /// CR-014: focused front, others stacked behind
    Stack,
}

impl Default for AlignMode {
    fn default() -> Self {
        AlignMode::Side
    }
}

/// Configuration for the side alignment layout (CR-001).
/// Focused window on the left, unfocused windows stacked
/// vertically on the right.
#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct SideAlign {
    /// Focused window width as a ratio of screen (0.1–1.0).
    #[serde(default = "default_side_align_width")]
    pub width: f32,
    /// Pixels of margin between windows.
    #[serde(default = "default_side_align_gap")]
    pub gap: u32,
    /// Reserved for future use (peek offset).
    #[serde(default = "default_side_peek_width", rename = "peek-width")]
    pub peek_width: u32,
    /// When true, layout only changes via keyboard shortcuts,
    /// ignoring mouse clicks and OS-triggered focus changes.
    #[serde(default = "bool::default", rename = "keyboard-only-focus")]
    pub keyboard_only_focus: bool,
}

fn default_side_align_width() -> f32 {
    1.0
}

fn default_side_align_gap() -> u32 {
    10
}

fn default_side_peek_width() -> u32 {
    50
}

impl Default for SideAlign {
    fn default() -> Self {
        Self {
            width: default_side_align_width(),
            gap: default_side_align_gap(),
            peek_width: default_side_peek_width(),
            keyboard_only_focus: false,
        }
    }
}

/// Configuration for the stack alignment layout (CR-014).
/// Focused window in front at near-full size, unfocused
/// windows arranged left-to-right behind it.
#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct StackAlign {
    /// Focused window width as a ratio of screen (0.1–1.0).
    #[serde(default = "default_stack_align_width")]
    pub width: f32,
    /// Focused window height as a ratio of screen (0.1–1.0).
    /// Only affects the focused window; unfocused windows use
    /// full screen height.
    #[serde(default = "default_stack_align_height")]
    pub height: f32,
    /// Pixels of margin between windows.
    #[serde(default = "default_stack_align_gap")]
    pub gap: u32,
    /// When true, layout only changes via keyboard shortcuts,
    /// ignoring mouse clicks and OS-triggered focus changes.
    #[serde(default = "bool::default", rename = "keyboard-only-focus")]
    pub keyboard_only_focus: bool,
    /// When true, unfocused windows in stack mode are sent to
    /// the macOS desktop wallpaper layer, placing them behind
    /// all normal applications. The focused window stays at the
    /// normal window level. On non-macOS platforms this is
    /// ignored.
    #[serde(default = "bool::default", rename = "wallpaper-back")]
    pub wallpaper_back: bool,
}

fn default_stack_align_width() -> f32 {
    1.0
}

fn default_stack_align_height() -> f32 {
    1.0
}

fn default_stack_align_gap() -> u32 {
    10
}

impl Default for StackAlign {
    fn default() -> Self {
        Self {
            width: default_stack_align_width(),
            height: default_stack_align_height(),
            gap: default_stack_align_gap(),
            keyboard_only_focus: false,
            wallpaper_back: false,
        }
    }
}

#[derive(PartialEq, Serialize, Deserialize, Clone, Debug)]
pub struct Window {
    #[serde(default = "default_window_width")]
    pub width: i32,
    #[serde(default = "default_window_height")]
    pub height: i32,
    #[serde(default = "WindowMode::default")]
    pub mode: WindowMode,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "bool::default")]
    pub blur: bool,
    #[serde(rename = "background-image", skip_serializing)]
    pub background_image: Option<ImageProperties>,
    #[serde(default = "Decorations::default")]
    pub decorations: Decorations,
    #[serde(default = "bool::default", rename = "macos-use-unified-titlebar")]
    pub macos_use_unified_titlebar: bool,
    #[serde(rename = "macos-use-shadow", default = "default_bool_true")]
    pub macos_use_shadow: bool,
    #[serde(rename = "initial-title", skip_serializing)]
    pub initial_title: Option<String>,
    #[serde(rename = "windows-use-undecorated-shadow", default = "Option::default")]
    pub windows_use_undecorated_shadow: Option<bool>,
    #[serde(
        rename = "windows-use-no-redirection-bitmap",
        default = "Option::default"
    )]
    pub windows_use_no_redirection_bitmap: Option<bool>,
    #[serde(rename = "windows-corner-preference", default = "Option::default")]
    pub windows_corner_preference: Option<WindowsCornerPreference>,
    #[serde(default = "Colorspace::default")]
    pub colorspace: Colorspace,
    /// Master toggle for automatic window alignment.
    #[serde(default = "bool::default", rename = "auto-align")]
    pub auto_align: bool,
    /// Default alignment mode: "side" (CR-001) or "stack" (CR-014).
    #[serde(default = "AlignMode::default", rename = "align-mode")]
    pub align_mode: AlignMode,
    /// Side alignment configuration (CR-001).
    #[serde(default = "SideAlign::default", rename = "side-align")]
    pub side_align: SideAlign,
    /// Stack alignment configuration (CR-014).
    #[serde(default = "StackAlign::default", rename = "stack-align")]
    pub stack_align: StackAlign,
    /// Glowing border effect around the window edges (Opera GX style).
    #[serde(default = "BorderGlow::default", rename = "border-glow")]
    pub border_glow: BorderGlow,
}

impl Default for Window {
    fn default() -> Window {
        Window {
            width: default_window_width(),
            height: default_window_height(),
            mode: WindowMode::default(),
            opacity: default_opacity(),
            background_image: None,
            decorations: Decorations::default(),
            blur: false,
            macos_use_unified_titlebar: false,
            macos_use_shadow: true,
            initial_title: None,
            windows_use_undecorated_shadow: None,
            windows_use_no_redirection_bitmap: None,
            windows_corner_preference: None,
            colorspace: Colorspace::default(),
            auto_align: false,
            align_mode: AlignMode::default(),
            side_align: SideAlign::default(),
            stack_align: StackAlign::default(),
            border_glow: BorderGlow::default(),
        }
    }
}

impl Colorspace {
    pub fn to_sugarloaf_colorspace(&self) -> sugarloaf::Colorspace {
        match self {
            Colorspace::Srgb => sugarloaf::Colorspace::Srgb,
            Colorspace::DisplayP3 => sugarloaf::Colorspace::DisplayP3,
            Colorspace::Rec2020 => sugarloaf::Colorspace::Rec2020,
        }
    }

    #[cfg(target_os = "macos")]
    pub fn to_rio_window_colorspace(&self) -> rio_window::platform::macos::Colorspace {
        match self {
            Colorspace::Srgb => rio_window::platform::macos::Colorspace::Srgb,
            Colorspace::DisplayP3 => rio_window::platform::macos::Colorspace::DisplayP3,
            Colorspace::Rec2020 => rio_window::platform::macos::Colorspace::Rec2020,
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn to_rio_window_colorspace(&self) {
        // No-op for non-macOS platforms
    }
}

impl Window {
    pub fn is_fullscreen(&self) -> bool {
        self.mode == WindowMode::Fullscreen
    }
}
