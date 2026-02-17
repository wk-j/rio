// Leader key modal menu configuration

use serde::{Deserialize, Serialize};

/// Leader key configuration (intermediate for deserialization)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Leader {
    /// Key combination to trigger the leader menu (e.g., "ctrl+space")
    #[serde(default = "default_leader_key")]
    pub key: String,

    /// Menu items from config (will be merged with defaults)
    #[serde(default)]
    items: Vec<LeaderItem>,
}

impl Default for Leader {
    fn default() -> Self {
        Self {
            key: default_leader_key(),
            items: Vec::new(),
        }
    }
}

impl Leader {
    /// Get the final list of items, merging config items with defaults.
    /// Config items override defaults with the same key.
    pub fn items(&self) -> Vec<LeaderItem> {
        let defaults = default_leader_items();

        if self.items.is_empty() {
            return defaults;
        }

        // Start with defaults, then override/add from config
        let mut result = defaults;

        for config_item in &self.items {
            // Check if this key already exists in defaults
            if let Some(pos) = result.iter().position(|item| item.key == config_item.key)
            {
                // Override the default
                result[pos] = config_item.clone();
            } else {
                // Add new item
                result.push(config_item.clone());
            }
        }

        result
    }
}

/// Parsed leader key binding
#[derive(Debug, Clone)]
pub struct ParsedLeaderKey {
    /// The key itself (e.g., "space", ";", "a")
    pub key: String,
    /// Whether CTRL modifier is required
    pub ctrl: bool,
    /// Whether ALT/OPTION modifier is required
    pub alt: bool,
    /// Whether SHIFT modifier is required
    pub shift: bool,
    /// Whether SUPER/CMD modifier is required
    pub super_key: bool,
}

impl Leader {
    /// Parse the leader key configuration string (e.g., "ctrl+space", "super+;")
    /// Returns the key and modifier flags
    pub fn parse_key(&self) -> ParsedLeaderKey {
        let mut result = ParsedLeaderKey {
            key: String::new(),
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
        };

        let parts: Vec<&str> = self.key.split('+').collect();
        for part in parts {
            match part.trim().to_lowercase().as_str() {
                "ctrl" | "control" => result.ctrl = true,
                "alt" | "option" => result.alt = true,
                "shift" => result.shift = true,
                "super" | "cmd" | "command" => result.super_key = true,
                key => result.key = key.to_string(),
            }
        }

        result
    }
}

fn default_leader_key() -> String {
    "super+;".to_string()
}

fn default_leader_items() -> Vec<LeaderItem> {
    vec![
        // Window/Tab management
        LeaderItem {
            key: 'n',
            label: "New window".to_string(),
            action: Some("WindowCreateNew".to_string()),
            write: None,
        },
        LeaderItem {
            key: 't',
            label: "New tab".to_string(),
            action: Some("TabCreateNew".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'x',
            label: "Close".to_string(),
            action: Some("CloseCurrentSplitOrTab".to_string()),
            write: None,
        },
        LeaderItem {
            key: '[',
            label: "Prev tab".to_string(),
            action: Some("SelectPrevTab".to_string()),
            write: None,
        },
        LeaderItem {
            key: ']',
            label: "Next tab".to_string(),
            action: Some("SelectNextTab".to_string()),
            write: None,
        },
        // Split creation
        LeaderItem {
            key: 's',
            label: "Split right".to_string(),
            action: Some("SplitRight".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'v',
            label: "Split down".to_string(),
            action: Some("SplitDown".to_string()),
            write: None,
        },
        // Pane navigation (vim-style h/j/k/l)
        LeaderItem {
            key: 'h',
            label: "Pane left".to_string(),
            action: Some("SelectSplitLeft".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'j',
            label: "Pane down".to_string(),
            action: Some("SelectSplitDown".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'k',
            label: "Pane up".to_string(),
            action: Some("SelectSplitUp".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'l',
            label: "Pane right".to_string(),
            action: Some("SelectSplitRight".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'z',
            label: "Zoom pane".to_string(),
            action: Some("ToggleZoom".to_string()),
            write: None,
        },
        // Other
        LeaderItem {
            key: 'y',
            label: "Copy mode".to_string(),
            action: Some("ToggleViMode".to_string()),
            write: None,
        },
        LeaderItem {
            key: '/',
            label: "Search".to_string(),
            action: Some("SearchForward".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'r',
            label: "Clear history".to_string(),
            action: Some("ClearHistory".to_string()),
            write: None,
        },
    ]
}

/// A single menu item in the leader menu
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LeaderItem {
    /// Key to press to trigger this item
    pub key: char,

    /// Display label
    pub label: String,

    /// Built-in Rio action to execute (e.g., "TabCreateNew")
    #[serde(default)]
    pub action: Option<String>,

    /// Text to write to PTY (as if user typed it)
    /// Supports variables: ${SELECTION}, ${WORD}, ${LINE}, ${CWD}, ${FILE}
    #[serde(default)]
    pub write: Option<String>,
}
