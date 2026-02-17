// Leader key modal menu state and handling

use crate::bindings::Action;
use rio_backend::config::leader::LeaderItem;

/// State of the leader menu
#[derive(Debug, Default)]
pub struct LeaderMenuState {
    /// Whether the menu is currently active/visible
    pub active: bool,
    /// Menu items from config
    pub items: Vec<LeaderItem>,
}

impl LeaderMenuState {
    pub fn new(items: Vec<LeaderItem>) -> Self {
        Self {
            active: false,
            items,
        }
    }

    /// Toggle the leader menu visibility
    pub fn toggle(&mut self) {
        self.active = !self.active;
    }

    /// Close the leader menu
    pub fn close(&mut self) {
        self.active = false;
    }

    /// Find item by key and return the action/write
    pub fn find_item(&self, key: char) -> Option<&LeaderItem> {
        self.items.iter().find(|item| item.key == key)
    }

    /// Parse action string to Action enum
    pub fn parse_action(action_str: &str) -> Action {
        Action::from(action_str.to_string())
    }
}
