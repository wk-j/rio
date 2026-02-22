use crate::event::SoundEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A sound entry can be a single path or a list of paths (variants).
/// When multiple paths are provided, they are rotated via round-robin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SoundPaths {
    Single(PathBuf),
    Multiple(Vec<PathBuf>),
}

impl SoundPaths {
    pub fn into_vec(self) -> Vec<PathBuf> {
        match self {
            SoundPaths::Single(p) => vec![p],
            SoundPaths::Multiple(v) => v,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SoundEffects {
    #[serde(default)]
    pub bell: Option<SoundPaths>,
    #[serde(default)]
    pub window_create: Option<SoundPaths>,
    #[serde(default)]
    pub window_close: Option<SoundPaths>,
    #[serde(default)]
    pub tab_create: Option<SoundPaths>,
    #[serde(default)]
    pub tab_close: Option<SoundPaths>,
    #[serde(default)]
    pub split_create: Option<SoundPaths>,
    #[serde(default)]
    pub split_close: Option<SoundPaths>,
    #[serde(default)]
    pub key_letter: Option<SoundPaths>,
    #[serde(default)]
    pub key_enter: Option<SoundPaths>,
    #[serde(default)]
    pub key_space: Option<SoundPaths>,
    #[serde(default)]
    pub key_backspace: Option<SoundPaths>,

    /// Global volume multiplier (0.0–1.0).
    #[serde(default = "default_volume")]
    pub volume: f32,

    /// Whether to play sounds at all.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Whether to play keyboard typing sounds (default off).
    #[serde(default = "default_keyboard_enabled")]
    pub keyboard_enabled: bool,

    /// Maximum duration in seconds for any single sound file.
    /// Files exceeding this are skipped during loading.
    #[serde(default = "default_max_duration")]
    pub max_duration: f32,
}

fn default_volume() -> f32 {
    0.7
}

fn default_enabled() -> bool {
    true
}

fn default_keyboard_enabled() -> bool {
    false
}

fn default_max_duration() -> f32 {
    5.0
}

impl Default for SoundEffects {
    fn default() -> Self {
        Self {
            bell: None,
            window_create: None,
            window_close: None,
            tab_create: None,
            tab_close: None,
            split_create: None,
            split_close: None,
            key_letter: None,
            key_enter: None,
            key_space: None,
            key_backspace: None,
            volume: default_volume(),
            enabled: default_enabled(),
            keyboard_enabled: default_keyboard_enabled(),
            max_duration: default_max_duration(),
        }
    }
}

/// Resolve a path that may start with `~` (home directory) or be
/// relative to the config directory.
fn resolve_path(path: PathBuf, config_dir: &std::path::Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") || s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(s.strip_prefix("~/").unwrap_or(""));
        }
    }
    if path.is_absolute() {
        path
    } else {
        config_dir.join(path)
    }
}

impl SoundEffects {
    /// Build a mapping from `SoundEvent` to resolved file paths.
    /// Only events with configured paths are included.
    /// Keyboard events are excluded when `keyboard_enabled` is false.
    pub fn build_mapping(
        &self,
        config_dir: &std::path::Path,
    ) -> HashMap<SoundEvent, Vec<PathBuf>> {
        let mut map = HashMap::new();

        let mut entries: Vec<(SoundEvent, &Option<SoundPaths>)> = vec![
            (SoundEvent::Bell, &self.bell),
            (SoundEvent::WindowCreate, &self.window_create),
            (SoundEvent::WindowClose, &self.window_close),
            (SoundEvent::TabCreate, &self.tab_create),
            (SoundEvent::TabClose, &self.tab_close),
            (SoundEvent::SplitCreate, &self.split_create),
            (SoundEvent::SplitClose, &self.split_close),
        ];

        if self.keyboard_enabled {
            entries.extend([
                (SoundEvent::KeyLetter, &self.key_letter),
                (SoundEvent::KeyEnter, &self.key_enter),
                (SoundEvent::KeySpace, &self.key_space),
                (SoundEvent::KeyBackspace, &self.key_backspace),
            ]);
        }

        for (event, opt) in entries {
            if let Some(paths) = opt {
                let resolved: Vec<PathBuf> = paths
                    .clone()
                    .into_vec()
                    .into_iter()
                    .map(|p| resolve_path(p, config_dir))
                    .collect();
                if !resolved.is_empty() {
                    map.insert(event, resolved);
                }
            }
        }

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_paths_into_vec_single() {
        let sp = SoundPaths::Single(PathBuf::from("bell.wav"));
        assert_eq!(sp.into_vec(), vec![PathBuf::from("bell.wav")]);
    }

    #[test]
    fn test_sound_paths_into_vec_multiple() {
        let sp =
            SoundPaths::Multiple(vec![PathBuf::from("k1.wav"), PathBuf::from("k2.wav")]);
        assert_eq!(
            sp.into_vec(),
            vec![PathBuf::from("k1.wav"), PathBuf::from("k2.wav")]
        );
    }

    #[test]
    fn test_default_sound_effects() {
        let se = SoundEffects::default();
        assert!(se.bell.is_none());
        assert!(se.window_create.is_none());
        assert!(se.key_letter.is_none());
        assert_eq!(se.volume, 0.7);
        assert!(se.enabled);
        assert!(!se.keyboard_enabled);
        assert_eq!(se.max_duration, 5.0);
    }

    #[test]
    fn test_empty_config_produces_empty_mapping() {
        let se = SoundEffects::default();
        let config_dir = std::path::Path::new("/tmp");
        let map = se.build_mapping(config_dir);
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_mapping_single_path() {
        let se = SoundEffects {
            bell: Some(SoundPaths::Single(PathBuf::from("/sounds/bell.wav"))),
            ..SoundEffects::default()
        };
        let config_dir = std::path::Path::new("/tmp");
        let map = se.build_mapping(config_dir);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(&SoundEvent::Bell).unwrap(),
            &vec![PathBuf::from("/sounds/bell.wav")]
        );
    }

    #[test]
    fn test_build_mapping_multiple_paths() {
        let se = SoundEffects {
            key_letter: Some(SoundPaths::Multiple(vec![
                PathBuf::from("/s/k1.wav"),
                PathBuf::from("/s/k2.wav"),
                PathBuf::from("/s/k3.wav"),
            ])),
            keyboard_enabled: true,
            ..SoundEffects::default()
        };
        let config_dir = std::path::Path::new("/tmp");
        let map = se.build_mapping(config_dir);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&SoundEvent::KeyLetter).unwrap().len(), 3);
    }

    #[test]
    fn test_build_mapping_relative_path() {
        let se = SoundEffects {
            bell: Some(SoundPaths::Single(PathBuf::from("sounds/bell.wav"))),
            ..SoundEffects::default()
        };
        let config_dir = std::path::Path::new("/home/user/.config/rio");
        let map = se.build_mapping(config_dir);
        assert_eq!(
            map.get(&SoundEvent::Bell).unwrap(),
            &vec![PathBuf::from("/home/user/.config/rio/sounds/bell.wav")]
        );
    }

    #[test]
    fn test_toml_deserialization_single() {
        let toml_str = r#"
            bell = "/tmp/bell.wav"
        "#;
        let se: SoundEffects = toml::from_str(toml_str).unwrap();
        assert_eq!(
            se.bell,
            Some(SoundPaths::Single(PathBuf::from("/tmp/bell.wav")))
        );
    }

    #[test]
    fn test_toml_deserialization_multiple() {
        let toml_str = r#"
            key-letter = ["/tmp/k1.wav", "/tmp/k2.wav"]
        "#;
        let se: SoundEffects = toml::from_str(toml_str).unwrap();
        assert_eq!(
            se.key_letter,
            Some(SoundPaths::Multiple(vec![
                PathBuf::from("/tmp/k1.wav"),
                PathBuf::from("/tmp/k2.wav"),
            ]))
        );
    }

    #[test]
    fn test_toml_deserialization_kebab_case() {
        let toml_str = r#"
            window-create = "/tmp/new_tab.mp3"
            key-backspace = "/tmp/bs.wav"
            keyboard-enabled = true
            max-duration = 3.0
        "#;
        let se: SoundEffects = toml::from_str(toml_str).unwrap();
        assert!(se.window_create.is_some());
        assert!(se.key_backspace.is_some());
        assert!(se.keyboard_enabled);
        assert_eq!(se.max_duration, 3.0);
    }

    #[test]
    fn test_toml_deserialization_defaults() {
        let toml_str = "";
        let se: SoundEffects = toml::from_str(toml_str).unwrap();
        assert_eq!(se.volume, 0.7);
        assert!(se.enabled);
        assert!(!se.keyboard_enabled);
        assert_eq!(se.max_duration, 5.0);
    }

    #[test]
    fn test_keyboard_events_excluded_when_disabled() {
        let se = SoundEffects {
            key_letter: Some(SoundPaths::Single(PathBuf::from("/s/k.wav"))),
            key_enter: Some(SoundPaths::Single(PathBuf::from("/s/e.wav"))),
            bell: Some(SoundPaths::Single(PathBuf::from("/s/bell.wav"))),
            keyboard_enabled: false,
            ..SoundEffects::default()
        };
        let config_dir = std::path::Path::new("/tmp");
        let map = se.build_mapping(config_dir);
        // Only bell should be in the mapping; keyboard events excluded
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&SoundEvent::Bell));
        assert!(!map.contains_key(&SoundEvent::KeyLetter));
        assert!(!map.contains_key(&SoundEvent::KeyEnter));
    }

    #[test]
    fn test_keyboard_events_included_when_enabled() {
        let se = SoundEffects {
            key_letter: Some(SoundPaths::Single(PathBuf::from("/s/k.wav"))),
            key_enter: Some(SoundPaths::Single(PathBuf::from("/s/e.wav"))),
            keyboard_enabled: true,
            ..SoundEffects::default()
        };
        let config_dir = std::path::Path::new("/tmp");
        let map = se.build_mapping(config_dir);
        assert_eq!(map.len(), 2);
        assert!(map.contains_key(&SoundEvent::KeyLetter));
        assert!(map.contains_key(&SoundEvent::KeyEnter));
    }
}
