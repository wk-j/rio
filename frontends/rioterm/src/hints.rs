use rio_backend::config::hints::Hint;
use rio_backend::crosswords::grid::Dimensions;
use rio_backend::crosswords::pos::{Column, Line, Pos};
use rio_backend::event::EventListener;
use std::rc::Rc;

/// State for hint selection mode
pub struct HintState {
    /// Currently active hint configuration
    active_hint: Option<Rc<Hint>>,

    /// Visible matches for the current hint
    matches: Vec<HintMatch>,

    /// Labels for each match (as Vec<char>)
    labels: Vec<Vec<char>>,

    /// Keys pressed so far for hint selection
    keys: Vec<char>,

    /// Alphabet for generating labels
    alphabet: String,
}

/// A match found by a hint
#[derive(Debug, Clone)]
pub struct HintMatch {
    /// The text that was matched
    pub text: String,

    /// Start position of the match
    pub start: Pos,

    /// End position of the match
    pub end: Pos,

    /// The hint configuration that created this match
    pub hint: Rc<Hint>,
}

impl HintState {
    pub fn new(alphabet: String) -> Self {
        Self {
            active_hint: None,
            matches: Vec::new(),
            labels: Vec::new(),
            keys: Vec::new(),
            alphabet,
        }
    }

    /// Check if hint mode is active
    pub fn is_active(&self) -> bool {
        self.active_hint.is_some()
    }

    /// Start hint mode with the given hint configuration
    pub fn start(&mut self, hint: Rc<Hint>) {
        self.active_hint = Some(hint);
        self.keys.clear();
        // matches and labels will be updated by update_matches
    }

    /// Stop hint mode
    pub fn stop(&mut self) {
        self.active_hint = None;
        self.matches.clear();
        self.labels.clear();
        self.keys.clear();
    }

    /// Update visible matches for the current hint
    pub fn update_matches<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
    ) {
        self.matches.clear();

        let hint = match &self.active_hint {
            Some(hint) => hint.clone(),
            None => {
                return;
            }
        };

        // Find regex matches if regex is specified
        if let Some(regex_pattern) = &hint.regex {
            if let Ok(regex) = regex::Regex::new(regex_pattern) {
                self.find_regex_matches(term, &regex, hint.clone());
            }
        }

        // Find OSC 8 hyperlinks if enabled
        if hint.hyperlinks {
            self.find_hyperlink_matches(term, hint.clone());
        }

        // Cancel hint mode if no matches found
        if self.matches.is_empty() {
            self.stop();
            return;
        }

        // Sort and dedup matches
        self.matches.sort_by_key(|m| (m.start.row, m.start.col));
        self.matches.dedup_by_key(|m| m.start);

        // Generate labels for matches
        self.generate_labels();
    }

    /// Handle keyboard input during hint selection
    pub fn keyboard_input<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
        c: char,
    ) -> Option<HintMatch> {
        match c {
            // Use backspace to remove the last character pressed
            '\x08' | '\x1f' => {
                self.keys.pop();
                // Only update matches after backspace to regenerate visible labels
                self.update_matches(term);
                return None;
            }
            // Cancel hint highlighting on ESC/Ctrl+c
            '\x1b' | '\x03' => {
                self.stop();
                return None;
            }
            _ => (),
        }

        let hint = self.active_hint.as_ref()?;

        // Get visible labels (labels filtered by keys pressed so far)
        let visible_labels = self.visible_labels();

        // Find a label starting with the input character
        let (index, remaining_label) = visible_labels
            .iter()
            .find(|(_, remaining)| !remaining.is_empty() && remaining[0] == c)?;

        // Check if this completes the label (only one character remaining)
        if remaining_label.len() == 1 {
            let hint_match = self.matches.get(*index)?.clone();
            let hint_config = hint.clone();

            // Exit hint mode unless it requires explicit dismissal
            if hint_config.persist {
                self.keys.clear();
            } else {
                self.stop();
            }

            Some(hint_match)
        } else {
            // Store character to preserve the selection
            self.keys.push(c);
            None
        }
    }

    /// Get current matches
    pub fn matches(&self) -> &[HintMatch] {
        &self.matches
    }

    /// Get keys pressed so far
    #[allow(dead_code)]
    pub fn keys_pressed(&self) -> &[char] {
        &self.keys
    }

    /// Get visible labels (filtered by current input)
    pub fn visible_labels(&self) -> Vec<(usize, Vec<char>)> {
        let keys_len = self.keys.len();
        self.labels
            .iter()
            .enumerate()
            .filter_map(|(i, label)| {
                if label.len() >= keys_len && label[..keys_len] == self.keys[..] {
                    let remaining: Vec<char> = label[keys_len..].to_vec();
                    Some((i, remaining))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Update the alphabet used for hint labels
    #[allow(dead_code)]
    pub fn update_alphabet(&mut self, alphabet: &str) {
        if self.alphabet != alphabet {
            self.alphabet = alphabet.to_string();
            self.keys.clear();
        }
    }

    // Private helper methods

    fn find_regex_matches<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
        regex: &regex::Regex,
        hint: Rc<Hint>,
    ) {
        // Get the visible area of the terminal
        let grid = &term.grid;
        let display_offset = grid.display_offset();
        let visible_lines = grid.screen_lines();

        // Scan each visible line for matches
        for line_idx in 0..visible_lines {
            let line = Line(line_idx as i32 - display_offset as i32);
            if line < Line(0) || line.0 >= grid.total_lines() as i32 {
                continue;
            }

            // Extract text from the line
            let line_text = self.extract_line_text(term, line);

            // Find all matches in this line
            // Use captures_iter to support capture groups: if the regex
            // contains a capture group, the first group is used as the
            // copied text while the full match defines the highlight range.
            for caps in regex.captures_iter(&line_text) {
                let full_match = caps.get(0).unwrap();
                let start_col = Column(full_match.start());

                // Use first capture group text if available, otherwise full match
                let mut match_text =
                    caps.get(1).unwrap_or(full_match).as_str().to_string();

                // Apply post-processing if enabled
                if hint.post_processing {
                    match_text = post_process_hyperlink_uri(&match_text);
                }

                // Highlight range is based on the full match
                let end_col =
                    Column(full_match.start() + full_match.len().saturating_sub(1));

                let hint_match = HintMatch {
                    text: match_text,
                    start: Pos::new(line, start_col),
                    end: Pos::new(line, end_col),
                    hint: hint.clone(),
                };

                self.matches.push(hint_match);
            }
        }
    }

    fn find_hyperlink_matches<T: EventListener>(
        &mut self,
        term: &rio_backend::crosswords::Crosswords<T>,
        hint: Rc<Hint>,
    ) {
        // Scan the visible area for OSC 8 hyperlinks
        let grid = &term.grid;
        let display_offset = grid.display_offset();
        let visible_lines = grid.screen_lines();

        for line_idx in 0..visible_lines {
            let line = Line(line_idx as i32 - display_offset as i32);
            if line < Line(0) || line.0 >= grid.total_lines() as i32 {
                continue;
            }

            let mut col = Column(0);
            while col < grid.columns() {
                let cell = &grid[line][col];

                if let Some(hyperlink) = cell.hyperlink() {
                    // Find the extent of this hyperlink
                    let start_col = col;
                    let mut end_col = col;

                    // Scan forward to find the end of the hyperlink
                    while end_col < grid.columns() {
                        let next_cell = &grid[line][end_col];
                        if next_cell.hyperlink().as_ref() == Some(&hyperlink) {
                            end_col += 1;
                        } else {
                            break;
                        }
                    }

                    let mut uri = hyperlink.uri().to_string();

                    // Apply post-processing if enabled
                    if hint.post_processing {
                        uri = post_process_hyperlink_uri(&uri);
                    }

                    let hint_match = HintMatch {
                        text: uri,
                        start: Pos::new(line, start_col),
                        end: Pos::new(line, end_col - 1),
                        hint: hint.clone(),
                    };

                    self.matches.push(hint_match);

                    // Skip to the end of this hyperlink
                    col = end_col;
                } else {
                    col += 1;
                }
            }
        }
    }

    fn extract_line_text<T: EventListener>(
        &self,
        term: &rio_backend::crosswords::Crosswords<T>,
        line: Line,
    ) -> String {
        let grid = &term.grid;
        let mut text = String::new();

        for col in 0..grid.columns() {
            let cell = &grid[line][Column(col)];
            text.push(cell.c);
        }

        text.trim_end().to_string()
    }

    fn generate_labels(&mut self) {
        self.labels.clear();
        let count = self.matches.len();
        let alphabet: Vec<char> = self.alphabet.chars().collect();
        let alphabet_len = alphabet.len();

        if count == 0 {
            return;
        }

        // Determine the minimum label length needed to have unique, non-prefix labels
        // We need labels where no label is a prefix of another
        // This means all labels must have the same length
        let label_len = if count <= alphabet_len {
            1
        } else {
            // Calculate minimum length needed: alphabet_len^length >= count
            let mut len = 2;
            let mut capacity = alphabet_len * alphabet_len;
            while capacity < count {
                len += 1;
                capacity *= alphabet_len;
            }
            len
        };

        // Generate labels of fixed length
        let mut indices = vec![0usize; label_len];

        for _ in 0..count {
            // Create label from current indices
            let label: Vec<char> = indices.iter().map(|&i| alphabet[i]).collect();
            self.labels.push(label);

            // Increment indices (like counting in base alphabet_len)
            let mut carry = true;
            for idx in indices.iter_mut().rev() {
                if carry {
                    *idx += 1;
                    if *idx >= alphabet_len {
                        *idx = 0;
                    } else {
                        carry = false;
                    }
                }
            }
        }
    }
}

/// Apply post-processing to hyperlink URIs (same as in screen/mod.rs)
fn post_process_hyperlink_uri(uri: &str) -> String {
    let chars: Vec<char> = uri.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut end_idx = chars.len() - 1;
    let mut open_parents = 0;
    let mut open_brackets = 0;

    // First pass: handle uneven brackets/parentheses
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => open_parents += 1,
            '[' => open_brackets += 1,
            ')' => {
                if open_parents == 0 {
                    // Unmatched closing parenthesis, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_parents -= 1;
                }
            }
            ']' => {
                if open_brackets == 0 {
                    // Unmatched closing bracket, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_brackets -= 1;
                }
            }
            _ => (),
        }
    }

    // Second pass: remove trailing delimiters
    while end_idx > 0 {
        match chars[end_idx] {
            '.' | ',' | ':' | ';' | '?' | '!' | '(' | '[' | '\'' => {
                end_idx = end_idx.saturating_sub(1);
            }
            _ => break,
        }
    }

    chars.into_iter().take(end_idx + 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::config::hints::{HintAction, HintInternalAction};

    #[test]
    fn test_label_generation() {
        let mut state = HintState::new("abc".to_string());

        // With 3 matches (fits in single char alphabet of 3)
        state.matches = vec![
            HintMatch {
                text: "m1".to_string(),
                start: Pos::new(Line(0), Column(0)),
                end: Pos::new(Line(0), Column(1)),
                hint: Rc::new(Hint {
                    regex: None,
                    hyperlinks: false,
                    post_processing: false,
                    persist: false,
                    action: HintAction::Action {
                        action: HintInternalAction::Copy,
                    },
                    mouse: Default::default(),
                    binding: None,
                }),
            },
            HintMatch {
                text: "m2".to_string(),
                start: Pos::new(Line(0), Column(5)),
                end: Pos::new(Line(0), Column(6)),
                hint: Rc::new(Hint {
                    regex: None,
                    hyperlinks: false,
                    post_processing: false,
                    persist: false,
                    action: HintAction::Action {
                        action: HintInternalAction::Copy,
                    },
                    mouse: Default::default(),
                    binding: None,
                }),
            },
            HintMatch {
                text: "m3".to_string(),
                start: Pos::new(Line(0), Column(10)),
                end: Pos::new(Line(0), Column(11)),
                hint: Rc::new(Hint {
                    regex: None,
                    hyperlinks: false,
                    post_processing: false,
                    persist: false,
                    action: HintAction::Action {
                        action: HintInternalAction::Copy,
                    },
                    mouse: Default::default(),
                    binding: None,
                }),
            },
        ];
        state.generate_labels();
        // 3 matches with alphabet "abc" (len 3) -> single char labels
        assert_eq!(state.labels.len(), 3);
        assert_eq!(state.labels[0], vec!['a']);
        assert_eq!(state.labels[1], vec!['b']);
        assert_eq!(state.labels[2], vec!['c']);

        // With 4 matches (exceeds alphabet size) -> 2 char labels
        state.matches.push(HintMatch {
            text: "m4".to_string(),
            start: Pos::new(Line(0), Column(15)),
            end: Pos::new(Line(0), Column(16)),
            hint: Rc::new(Hint {
                regex: None,
                hyperlinks: false,
                post_processing: false,
                persist: false,
                action: HintAction::Action {
                    action: HintInternalAction::Copy,
                },
                mouse: Default::default(),
                binding: None,
            }),
        });
        state.generate_labels();
        // 4 matches with alphabet "abc" (len 3) -> need 2 char labels (3^2 = 9 >= 4)
        assert_eq!(state.labels.len(), 4);
        assert_eq!(state.labels[0], vec!['a', 'a']);
        assert_eq!(state.labels[1], vec!['a', 'b']);
        assert_eq!(state.labels[2], vec!['a', 'c']);
        assert_eq!(state.labels[3], vec!['b', 'a']);
    }

    #[test]
    fn test_hint_state_lifecycle() {
        let mut state = HintState::new("abc".to_string());
        assert!(!state.is_active());

        let hint = Rc::new(Hint {
            regex: Some("test".to_string()),
            hyperlinks: false,
            post_processing: true,
            persist: false,
            action: HintAction::Action {
                action: HintInternalAction::Copy,
            },
            mouse: Default::default(),
            binding: None,
        });

        state.start(hint);
        assert!(state.is_active());

        state.stop();
        assert!(!state.is_active());
    }

    #[test]
    fn test_visible_labels() {
        let mut state = HintState::new("abc".to_string());
        state.labels = vec![vec!['a'], vec!['b'], vec!['a', 'b'], vec!['a', 'c']];

        // No input - all labels visible
        let visible = state.visible_labels();
        assert_eq!(visible.len(), 4);

        // Input "a" - should show labels that start with "a"
        state.keys = vec!['a'];
        let visible = state.visible_labels();
        assert_eq!(visible.len(), 3); // "a", "ab", "ac"
        assert_eq!(visible[0].1, Vec::<char>::new()); // "a" with "a" removed = []
        assert_eq!(visible[1].1, vec!['b']); // "ab" with "a" removed = ['b']
        assert_eq!(visible[2].1, vec!['c']); // "ac" with "a" removed = ['c']
    }

    #[test]
    fn test_keyboard_input_logic() {
        let mut state = HintState::new("jfkdls".to_string());

        // Simulate having some labels
        state.labels = vec![
            vec!['j'], // index 0
            vec!['f'], // index 1
            vec!['k'], // index 2
            vec!['d'], // index 3
            vec!['l'], // index 4
            vec!['s'], // index 5
        ];

        // Simulate having matches (we'll use dummy matches)
        state.matches = vec![
            HintMatch {
                text: "match0".to_string(),
                start: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(0),
                ),
                end: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(5),
                ),
                hint: Rc::new(Hint {
                    regex: Some("test".to_string()),
                    hyperlinks: false,
                    post_processing: true,
                    persist: false,
                    action: HintAction::Action {
                        action: HintInternalAction::Copy,
                    },
                    mouse: Default::default(),
                    binding: None,
                }),
            },
            HintMatch {
                text: "match1".to_string(),
                start: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(10),
                ),
                end: rio_backend::crosswords::pos::Pos::new(
                    rio_backend::crosswords::pos::Line(0),
                    rio_backend::crosswords::pos::Column(15),
                ),
                hint: Rc::new(Hint {
                    regex: Some("test".to_string()),
                    hyperlinks: false,
                    post_processing: true,
                    persist: false,
                    action: HintAction::Action {
                        action: HintInternalAction::Copy,
                    },
                    mouse: Default::default(),
                    binding: None,
                }),
            },
        ];

        let hint = Rc::new(Hint {
            regex: Some("test".to_string()),
            hyperlinks: false,
            post_processing: true,
            persist: false,
            action: HintAction::Action {
                action: HintInternalAction::Copy,
            },
            mouse: Default::default(),
            binding: None,
        });

        state.active_hint = Some(hint);

        // Test keyboard input logic without needing a terminal
        // Test that 'j' should match the first label
        let mut test_keys = state.keys.clone();
        test_keys.push('j');

        let mut matching_indices = Vec::new();
        for (i, label) in state.labels.iter().enumerate() {
            if label.len() >= test_keys.len() && label[..test_keys.len()] == test_keys[..]
            {
                matching_indices.push(i);
            }
        }

        assert!(
            !matching_indices.is_empty(),
            "Should find matching labels for 'j'"
        );
        assert_eq!(matching_indices, vec![0], "Should match index 0 for 'j'");

        // Test that the label should be completed (single character)
        let index = *matching_indices.last().unwrap();
        let label = &state.labels[index];
        assert_eq!(
            label.len(),
            test_keys.len(),
            "Label should be completed with single character"
        );
    }

    #[test]
    fn test_regex_multiple_matches_single_line() {
        // Inline backtick code pattern
        let regex = regex::Regex::new("`[^`]+`").unwrap();
        let line = "Use `git commit` and then `git push` to publish";

        let matches: Vec<&str> = regex.find_iter(line).map(|m| m.as_str()).collect();
        assert_eq!(matches, vec!["`git commit`", "`git push`"]);
    }

    #[test]
    fn test_regex_combined_patterns_single_line() {
        // Combined pattern: backtick code OR quoted strings OR file paths
        let regex =
            regex::Regex::new(r#"`[^`]+`|"[^"]+"|/?(?:[\w.-]+/)+[\w.-]+"#).unwrap();

        let line = r#"Run `cargo test` on "main.rs" at src/hints.rs"#;
        let matches: Vec<&str> = regex.find_iter(line).map(|m| m.as_str()).collect();
        assert_eq!(matches, vec!["`cargo test`", "\"main.rs\"", "src/hints.rs"]);
    }

    #[test]
    fn test_regex_no_matches() {
        let regex = regex::Regex::new("`[^`]+`").unwrap();
        let line = "No backtick code here";

        let matches: Vec<&str> = regex.find_iter(line).map(|m| m.as_str()).collect();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_regex_adjacent_matches() {
        let regex = regex::Regex::new("`[^`]+`").unwrap();
        let line = "`a``b``c`";

        let matches: Vec<&str> = regex.find_iter(line).map(|m| m.as_str()).collect();
        assert_eq!(matches, vec!["`a`", "`b`", "`c`"]);
    }
}
