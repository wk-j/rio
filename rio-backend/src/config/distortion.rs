use serde::{Deserialize, Serialize};

/// Distortion effect type applied to the rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DistortionType {
    #[default]
    None,
    /// Perspective tilt (vanishing point effect)
    Perspective,
}

/// Configuration for the `[distortion]` TOML section.
///
/// ```toml
/// [distortion]
/// effect = "perspective"
/// strength = 0.3
/// center = [0.5, 0.5]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DistortionConfig {
    /// Effect type (default: none)
    #[serde(default)]
    pub effect: DistortionType,

    /// Effect strength, 0.0 = no distortion, 1.0 = maximum.
    /// Negative values invert the effect. Default: 0.3
    #[serde(default = "default_strength")]
    pub strength: f32,

    /// Distortion center point in normalized coordinates
    /// (0.0–1.0). Default: [0.5, 0.5] (screen center).
    #[serde(default = "default_center")]
    pub center: [f32; 2],
}

fn default_strength() -> f32 {
    0.3
}

fn default_center() -> [f32; 2] {
    [0.5, 0.5]
}

impl Default for DistortionConfig {
    fn default() -> Self {
        Self {
            effect: DistortionType::None,
            strength: default_strength(),
            center: default_center(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distortion_default() {
        let config = DistortionConfig::default();
        assert_eq!(config.effect, DistortionType::None);
        assert_eq!(config.strength, 0.3);
        assert_eq!(config.center, [0.5, 0.5]);
    }

    #[test]
    fn test_distortion_perspective_toml() {
        let toml_str = r#"
            effect = "perspective"
            strength = 0.5
            center = [0.3, 0.7]
        "#;
        let config: DistortionConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.effect, DistortionType::Perspective);
        assert_eq!(config.strength, 0.5);
        assert_eq!(config.center, [0.3, 0.7]);
    }

    #[test]
    fn test_distortion_none_toml() {
        let toml_str = r#"
            effect = "none"
        "#;
        let config: DistortionConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.effect, DistortionType::None);
        assert_eq!(config.strength, 0.3);
    }

    #[test]
    fn test_distortion_empty_section() {
        let toml_str = "";
        let config: DistortionConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.effect, DistortionType::None);
    }
}
