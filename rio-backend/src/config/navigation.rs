use crate::config::colors::{deserialize_to_arr, ColorArray};
use crate::config::default_bool_true;
use serde::{Deserialize, Serialize};

// Default functions for BookmarkStyle fields
#[inline]
fn default_bookmark_width() -> f32 {
    15.0
}

#[inline]
fn default_bookmark_height_active() -> f32 {
    #[cfg(target_os = "macos")]
    {
        26.0
    }
    #[cfg(not(target_os = "macos"))]
    {
        8.0
    }
}

#[inline]
fn default_bookmark_height_inactive() -> f32 {
    #[cfg(target_os = "macos")]
    {
        16.0
    }
    #[cfg(not(target_os = "macos"))]
    {
        4.0
    }
}

#[inline]
fn default_bookmark_spacing() -> f32 {
    20.0
}

#[inline]
fn default_bookmark_padding_x() -> f32 {
    30.0
}

#[inline]
fn default_bookmark_border_radius() -> f32 {
    4.0
}

#[inline]
fn default_bookmark_border_width() -> f32 {
    0.0
}

#[inline]
fn default_bookmark_hue_rotation_enabled() -> bool {
    false
}

#[inline]
fn default_bookmark_base_hue() -> f32 {
    0.0
}

#[inline]
fn default_bookmark_saturation() -> f32 {
    0.7
}

#[inline]
fn default_bookmark_lightness_active() -> f32 {
    0.65
}

#[inline]
fn default_bookmark_lightness_inactive() -> f32 {
    0.35
}

#[inline]
fn default_bookmark_hue_step() -> f32 {
    40.0
}

#[inline]
fn default_bookmark_shadow_blur_radius() -> f32 {
    0.0
}

#[inline]
fn default_bookmark_shadow_offset() -> [f32; 2] {
    [0.0, 1.0]
}

/// Style configuration for bookmark-mode tab indicators.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BookmarkStyle {
    /// Width of each bookmark indicator (default: 15.0)
    #[serde(default = "default_bookmark_width")]
    pub width: f32,

    /// Height of the active bookmark indicator
    #[serde(default = "default_bookmark_height_active", rename = "height-active")]
    pub height_active: f32,

    /// Height of inactive bookmark indicators
    #[serde(
        default = "default_bookmark_height_inactive",
        rename = "height-inactive"
    )]
    pub height_inactive: f32,

    /// Spacing between bookmark indicators (default: 20.0)
    #[serde(default = "default_bookmark_spacing")]
    pub spacing: f32,

    /// Right-edge padding offset (default: 30.0)
    #[serde(default = "default_bookmark_padding_x", rename = "padding-x")]
    pub padding_x: f32,

    /// Corner rounding radius (default: 4.0, set 0.0 for sharp corners)
    #[serde(default = "default_bookmark_border_radius", rename = "border-radius")]
    pub border_radius: f32,

    /// Border width (default: 0.0 = no border)
    #[serde(default = "default_bookmark_border_width", rename = "border-width")]
    pub border_width: f32,

    /// Border color as hex string (optional, uses tab color if not set)
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_bookmark_border_color",
        rename = "border-color"
    )]
    pub border_color: ColorArray,

    /// Shadow blur radius (default: 0.0 = no shadow)
    #[serde(
        default = "default_bookmark_shadow_blur_radius",
        rename = "shadow-blur-radius"
    )]
    pub shadow_blur_radius: f32,

    /// Shadow color as hex string
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_bookmark_shadow_color",
        rename = "shadow-color"
    )]
    pub shadow_color: ColorArray,

    /// Shadow offset [x, y] (default: [0.0, 1.0])
    #[serde(default = "default_bookmark_shadow_offset", rename = "shadow-offset")]
    pub shadow_offset: [f32; 2],

    /// Enable per-tab hue rotation colors (default: false)
    #[serde(
        default = "default_bookmark_hue_rotation_enabled",
        rename = "hue-rotation"
    )]
    pub hue_rotation: bool,

    /// Starting hue in degrees 0-360 (default: 0.0 = red)
    #[serde(default = "default_bookmark_base_hue", rename = "base-hue")]
    pub base_hue: f32,

    /// Hue step in degrees between each tab (default: 40.0)
    #[serde(default = "default_bookmark_hue_step", rename = "hue-step")]
    pub hue_step: f32,

    /// Color saturation 0.0-1.0 (default: 0.7)
    #[serde(default = "default_bookmark_saturation")]
    pub saturation: f32,

    /// Lightness for the active tab 0.0-1.0 (default: 0.65)
    #[serde(
        default = "default_bookmark_lightness_active",
        rename = "lightness-active"
    )]
    pub lightness_active: f32,

    /// Lightness for inactive tabs 0.0-1.0 (default: 0.35)
    #[serde(
        default = "default_bookmark_lightness_inactive",
        rename = "lightness-inactive"
    )]
    pub lightness_inactive: f32,
}

#[inline]
fn default_bookmark_border_color() -> ColorArray {
    [0.0, 0.0, 0.0, 0.0]
}

#[inline]
fn default_bookmark_shadow_color() -> ColorArray {
    [0.0, 0.0, 0.0, 0.4]
}

impl Default for BookmarkStyle {
    fn default() -> Self {
        BookmarkStyle {
            width: default_bookmark_width(),
            height_active: default_bookmark_height_active(),
            height_inactive: default_bookmark_height_inactive(),
            spacing: default_bookmark_spacing(),
            padding_x: default_bookmark_padding_x(),
            border_radius: default_bookmark_border_radius(),
            border_width: default_bookmark_border_width(),
            border_color: default_bookmark_border_color(),
            shadow_blur_radius: default_bookmark_shadow_blur_radius(),
            shadow_color: default_bookmark_shadow_color(),
            shadow_offset: default_bookmark_shadow_offset(),
            hue_rotation: default_bookmark_hue_rotation_enabled(),
            base_hue: default_bookmark_base_hue(),
            hue_step: default_bookmark_hue_step(),
            saturation: default_bookmark_saturation(),
            lightness_active: default_bookmark_lightness_active(),
            lightness_inactive: default_bookmark_lightness_inactive(),
        }
    }
}

/// Convert HSL to linear RGB color array [r, g, b, a] with values in 0.0-1.0.
#[inline]
pub fn hsl_to_rgba(hue: f32, saturation: f32, lightness: f32, alpha: f32) -> [f32; 4] {
    let h = ((hue % 360.0) + 360.0) % 360.0;
    let s = saturation.clamp(0.0, 1.0);
    let l = lightness.clamp(0.0, 1.0);

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    [r1 + m, g1 + m, b1 + m, alpha]
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum NavigationMode {
    #[serde(alias = "plain")]
    Plain,
    #[serde(alias = "toptab")]
    TopTab,
    #[cfg(target_os = "macos")]
    #[serde(alias = "nativetab")]
    NativeTab,
    #[serde(alias = "bottomtab")]
    BottomTab,
    #[serde(alias = "bookmark")]
    Bookmark,
}

#[allow(clippy::derivable_impls)]
impl Default for NavigationMode {
    fn default() -> NavigationMode {
        #[cfg(target_os = "macos")]
        {
            NavigationMode::NativeTab
        }

        #[cfg(not(target_os = "macos"))]
        NavigationMode::Bookmark
    }
}

impl NavigationMode {
    const PLAIN_STR: &'static str = "Plain";
    const COLLAPSED_TAB_STR: &'static str = "Bookmark";
    const TOP_TAB_STR: &'static str = "TopTab";
    const BOTTOM_TAB_STR: &'static str = "BottomTab";
    #[cfg(target_os = "macos")]
    const NATIVE_TAB_STR: &'static str = "NativeTab";

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => Self::PLAIN_STR,
            Self::Bookmark => Self::COLLAPSED_TAB_STR,
            Self::TopTab => Self::TOP_TAB_STR,
            Self::BottomTab => Self::BOTTOM_TAB_STR,
            #[cfg(target_os = "macos")]
            Self::NativeTab => Self::NATIVE_TAB_STR,
        }
    }
}

#[inline]
pub fn modes_as_vec_string() -> Vec<String> {
    [
        NavigationMode::Plain,
        NavigationMode::Bookmark,
        NavigationMode::TopTab,
        NavigationMode::BottomTab,
        #[cfg(target_os = "macos")]
        NavigationMode::NativeTab,
    ]
    .iter()
    .map(|navigation_mode| navigation_mode.to_string())
    .collect()
}

impl std::fmt::Display for NavigationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseNavigationModeError;

impl std::str::FromStr for NavigationMode {
    type Err = ParseNavigationModeError;

    fn from_str(s: &str) -> Result<NavigationMode, ParseNavigationModeError> {
        match s {
            Self::COLLAPSED_TAB_STR => Ok(NavigationMode::Bookmark),
            Self::TOP_TAB_STR => Ok(NavigationMode::TopTab),
            Self::BOTTOM_TAB_STR => Ok(NavigationMode::BottomTab),
            #[cfg(target_os = "macos")]
            Self::NATIVE_TAB_STR => Ok(NavigationMode::NativeTab),
            Self::PLAIN_STR => Ok(NavigationMode::Plain),
            _ => Ok(NavigationMode::default()),
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ColorAutomation {
    #[serde(default = "String::new")]
    pub program: String,
    #[serde(default = "String::new")]
    pub path: String,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "crate::config::colors::defaults::tabs"
    )]
    pub color: ColorArray,
}

#[inline]
pub fn default_unfocused_split_opacity() -> f32 {
    0.4
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Navigation {
    #[serde(default = "NavigationMode::default")]
    pub mode: NavigationMode,
    #[serde(
        default = "Vec::default",
        rename = "color-automation",
        skip_serializing
    )]
    pub color_automation: Vec<ColorAutomation>,
    #[serde(default = "bool::default", skip_serializing)]
    pub clickable: bool,
    #[serde(
        default = "default_bool_true",
        rename = "current-working-directory",
        alias = "cwd"
    )]
    pub current_working_directory: bool,
    #[serde(default = "bool::default", rename = "use-terminal-title")]
    pub use_terminal_title: bool,
    #[serde(default = "default_bool_true", rename = "hide-if-single")]
    pub hide_if_single: bool,
    #[serde(default = "default_bool_true", rename = "use-split")]
    pub use_split: bool,
    #[serde(default = "default_bool_true", rename = "open-config-with-split")]
    pub open_config_with_split: bool,
    #[serde(
        default = "default_unfocused_split_opacity",
        rename = "unfocused-split-opacity"
    )]
    pub unfocused_split_opacity: f32,
    #[serde(default = "BookmarkStyle::default", rename = "bookmark-style")]
    pub bookmark_style: BookmarkStyle,
}

impl Default for Navigation {
    fn default() -> Navigation {
        Navigation {
            mode: NavigationMode::default(),
            color_automation: Vec::default(),
            clickable: false,
            current_working_directory: true,
            use_terminal_title: false,
            hide_if_single: true,
            use_split: true,
            unfocused_split_opacity: default_unfocused_split_opacity(),
            open_config_with_split: true,
            bookmark_style: BookmarkStyle::default(),
        }
    }
}

impl Navigation {
    #[inline]
    pub fn is_collapsed_mode(&self) -> bool {
        self.mode == NavigationMode::Bookmark
    }

    #[inline]
    pub fn is_placed_on_bottom(&self) -> bool {
        self.mode == NavigationMode::BottomTab
    }

    #[inline]
    pub fn is_native(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.mode == NavigationMode::NativeTab
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    #[inline]
    pub fn has_navigation_key_bindings(&self) -> bool {
        self.mode != NavigationMode::Plain
    }

    #[inline]
    pub fn is_placed_on_top(&self) -> bool {
        self.mode == NavigationMode::TopTab
    }
}

#[cfg(test)]
mod tests {
    use crate::config::colors::hex_to_color_arr;
    use crate::config::navigation::{Navigation, NavigationMode};
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct Root {
        #[serde(default = "Navigation::default")]
        navigation: Navigation,
    }

    #[test]
    fn test_collapsed_tab() {
        let content = r#"
            [navigation]
            mode = 'Bookmark'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Bookmark);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_top_tab() {
        let content = r#"
            [navigation]
            mode = 'TopTab'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::TopTab);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_bottom_tab() {
        let content = r#"
            [navigation]
            mode = 'BottomTab'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::BottomTab);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_color_automation() {
        let content = r#"
            [navigation]
            mode = 'Bookmark'
            color-automation = [
                { program = 'vim', color = '#333333' }
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Bookmark);
        assert!(!decoded.navigation.clickable);
        assert!(!decoded.navigation.color_automation.is_empty());
        assert_eq!(
            decoded.navigation.color_automation[0].program,
            "vim".to_string()
        );
        assert_eq!(decoded.navigation.color_automation[0].path, String::new());
        assert_eq!(
            decoded.navigation.color_automation[0].color,
            hex_to_color_arr("#333333")
        );
    }

    #[test]
    fn test_color_automation_arr() {
        let content = r#"
            [navigation]
            mode = 'BottomTab'
            color-automation = [
                { program = 'ssh', color = '#F1F1F1' },
                { program = 'tmux', color = '#333333' },
                { path = '/home', color = '#ffffff' },
                { program = 'nvim', path = '/usr', color = '#00b952' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::BottomTab);
        assert!(!decoded.navigation.clickable);
        assert!(!decoded.navigation.color_automation.is_empty());

        assert_eq!(
            decoded.navigation.color_automation[0].program,
            "ssh".to_string()
        );
        assert_eq!(decoded.navigation.color_automation[0].path, String::new());
        assert_eq!(
            decoded.navigation.color_automation[0].color,
            hex_to_color_arr("#F1F1F1")
        );

        assert_eq!(
            decoded.navigation.color_automation[1].program,
            "tmux".to_string()
        );
        assert_eq!(decoded.navigation.color_automation[1].path, String::new());
        assert_eq!(
            decoded.navigation.color_automation[1].color,
            hex_to_color_arr("#333333")
        );

        assert_eq!(
            decoded.navigation.color_automation[2].program,
            String::new()
        );
        assert_eq!(
            decoded.navigation.color_automation[2].path,
            "/home".to_string()
        );
        assert_eq!(
            decoded.navigation.color_automation[2].color,
            hex_to_color_arr("#ffffff")
        );

        assert_eq!(
            decoded.navigation.color_automation[3].program,
            "nvim".to_string()
        );
        assert_eq!(
            decoded.navigation.color_automation[3].path,
            "/usr".to_string()
        );
        assert_eq!(
            decoded.navigation.color_automation[3].color,
            hex_to_color_arr("#00b952")
        );
    }
}
