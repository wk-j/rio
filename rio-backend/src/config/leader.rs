// Leader key modal menu configuration

use serde::{Deserialize, Serialize};

/// Leader key configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Leader {
    /// Key combination to trigger the leader menu (e.g., "ctrl+space")
    #[serde(default = "default_leader_key")]
    pub key: String,

    /// Menu items
    #[serde(default = "default_leader_items")]
    pub items: Vec<LeaderItem>,
}

impl Default for Leader {
    fn default() -> Self {
        Self {
            key: default_leader_key(),
            items: default_leader_items(),
        }
    }
}

fn default_leader_key() -> String {
    "ctrl+space".to_string()
}

fn default_leader_items() -> Vec<LeaderItem> {
    vec![
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
            label: "Close tab".to_string(),
            action: Some("TabCloseCurrent".to_string()),
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
        LeaderItem {
            key: 'v',
            label: "Split down".to_string(),
            action: Some("SplitDown".to_string()),
            write: None,
        },
        LeaderItem {
            key: 'h',
            label: "Split right".to_string(),
            action: Some("SplitRight".to_string()),
            write: None,
        },
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
            label: "Reset".to_string(),
            action: Some("ResetTerminal".to_string()),
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
