//! Shared terminal hint policy.
//!
//! Desktop and web still own terminal grid access and hint side effects.
//! This module owns portable hint selection behavior: label generation and
//! filtering the labels visible after the keys typed so far.

use crate::editor::selection_model::post_process_hyperlink_uri;
use neoism_terminal_core::crosswords::grid::Dimensions;
use neoism_terminal_core::crosswords::pos::{Column, Line, Pos};
use neoism_terminal_core::crosswords::Crosswords;

/// Generates hint labels using the same least-significant-index first counter
/// as the desktop terminal hint mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HintLabelGenerator {
    alphabet: Vec<char>,
    indices: Vec<usize>,
}

impl HintLabelGenerator {
    pub fn new(alphabet: &str) -> Self {
        Self {
            alphabet: alphabet.chars().collect(),
            indices: vec![0],
        }
    }

    pub fn next_label(&mut self) -> Option<Vec<char>> {
        if self.alphabet.is_empty() {
            return None;
        }

        let label = self.current_label();
        self.increment();
        Some(label)
    }

    fn current_label(&self) -> Vec<char> {
        self.indices
            .iter()
            .rev()
            .map(|&i| self.alphabet[i])
            .collect()
    }

    fn increment(&mut self) {
        let mut carry = true;
        let mut pos = 0;

        while carry && pos < self.indices.len() {
            self.indices[pos] += 1;
            if self.indices[pos] >= self.alphabet.len() {
                self.indices[pos] = 0;
                pos += 1;
            } else {
                carry = false;
            }
        }

        if carry {
            self.indices.push(0);
        }
    }
}

pub fn generate_hint_labels(alphabet: &str, count: usize) -> Vec<Vec<char>> {
    let mut generator = HintLabelGenerator::new(alphabet);
    (0..count).filter_map(|_| generator.next_label()).collect()
}

pub fn visible_hint_labels(
    labels: &[Vec<char>],
    keys_pressed: &[char],
) -> Vec<(usize, Vec<char>)> {
    let keys_len = keys_pressed.len();
    labels
        .iter()
        .enumerate()
        .filter_map(|(index, label)| {
            if label.len() >= keys_len && label[..keys_len] == keys_pressed[..] {
                Some((index, label[keys_len..].to_vec()))
            } else {
                None
            }
        })
        .collect()
}

pub fn sort_dedup_hint_matches_by_start<T>(
    matches: &mut Vec<T>,
    start: impl Fn(&T) -> Pos + Copy,
) {
    matches.sort_by_key(|hint_match| {
        let pos = start(hint_match);
        (pos.row, pos.col)
    });
    matches.dedup_by_key(|hint_match| start(hint_match));
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileLinkToken {
    pub col_start: usize,
    pub col_end: usize,
    pub text: String,
}

/// Classify the terminal row token under `col` as a possible local file link.
///
/// This is intentionally filesystem-free: hosts still resolve the returned
/// text against their current working directory and decide how to open it.
pub fn terminal_file_link_token_at(row_text: &str, col: usize) -> Option<FileLinkToken> {
    let chars: Vec<char> = row_text.chars().collect();
    if col >= chars.len() || is_file_link_delimiter(chars[col]) {
        return None;
    }

    let mut start = col;
    while start > 0 && !is_file_link_delimiter(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < chars.len() && !is_file_link_delimiter(chars[end]) {
        end += 1;
    }

    let text: String = chars[start..end].iter().collect();
    let text = text
        .trim_end_matches(|c: char| matches!(c, ':' | '.' | ','))
        .to_string();
    if text.is_empty() {
        return None;
    }

    Some(FileLinkToken {
        col_start: start,
        col_end: end,
        text,
    })
}

#[inline]
fn is_file_link_delimiter(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '\'' | '"' | '(' | ')' | '[' | ']' | '<' | '>' | '|' | '*' | '?' | ';' | ','
        )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HintTextMatch {
    pub text: String,
    pub start: Pos,
    pub end: Pos,
}

pub fn visible_hyperlink_hint_matches(
    terminal: &Crosswords,
    post_process: bool,
) -> Vec<HintTextMatch> {
    let grid = &terminal.grid;
    let display_offset = grid.display_offset();
    let visible_lines = grid.screen_lines();
    let mut matches = Vec::new();

    for line_idx in 0..visible_lines {
        let line = Line(line_idx as i32 - display_offset as i32);
        if line < Line(0) || line.0 >= grid.total_lines() as i32 {
            continue;
        }

        let mut col = 0usize;
        let cols = grid.columns();
        while col < cols {
            let id = match terminal.cell_hyperlink_id(line, Column(col)) {
                Some(id) => id,
                None => {
                    col += 1;
                    continue;
                }
            };

            let start_col = col;
            let mut end_col = col;
            while end_col < cols
                && terminal.cell_hyperlink_id(line, Column(end_col)) == Some(id)
            {
                end_col += 1;
            }

            if let Some(hyperlink) = terminal.cell_hyperlink(line, Column(start_col)) {
                let text = if post_process {
                    post_process_hyperlink_uri(hyperlink.uri())
                } else {
                    hyperlink.uri().to_string()
                };
                matches.push(HintTextMatch {
                    text,
                    start: Pos::new(line, Column(start_col)),
                    end: Pos::new(line, Column(end_col - 1)),
                });
            }

            col = end_col;
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoism_terminal_core::ansi::CursorShape;
    use neoism_terminal_core::crosswords::pos::{Column, Line};
    use neoism_terminal_core::crosswords::CrosswordsSize;
    use neoism_terminal_core::handler::{Processor, StdSyncHandler};

    fn pos(row: i32, col: usize) -> Pos {
        Pos::new(Line(row), Column(col))
    }

    fn terminal_with_osc8(bytes: &[u8]) -> Crosswords {
        let mut terminal = Crosswords::new(
            CrosswordsSize::new(40, 5),
            CursorShape::Block,
            neoism_terminal_core::TerminalId::new(0),
            10_000,
        );
        let mut processor = Processor::<StdSyncHandler>::new();
        processor.advance(&mut terminal, bytes);
        terminal
    }

    #[test]
    fn label_generator_matches_desktop_sequence() {
        let mut gen = HintLabelGenerator::new("abc");

        assert_eq!(gen.next_label(), Some(vec!['a']));
        assert_eq!(gen.next_label(), Some(vec!['b']));
        assert_eq!(gen.next_label(), Some(vec!['c']));
        assert_eq!(gen.next_label(), Some(vec!['a', 'a']));
        assert_eq!(gen.next_label(), Some(vec!['a', 'b']));
        assert_eq!(gen.next_label(), Some(vec!['a', 'c']));
        assert_eq!(gen.next_label(), Some(vec!['b', 'a']));
    }

    #[test]
    fn generate_hint_labels_limits_to_requested_count() {
        assert_eq!(
            generate_hint_labels("ab", 5),
            vec![
                vec!['a'],
                vec!['b'],
                vec!['a', 'a'],
                vec!['a', 'b'],
                vec!['b', 'a'],
            ]
        );
        assert!(generate_hint_labels("ab", 0).is_empty());
        assert!(generate_hint_labels("", 4).is_empty());
    }

    #[test]
    fn visible_hint_labels_filters_prefix_and_strips_entered_keys() {
        let labels = vec![
            vec!['a'],
            vec!['b'],
            vec!['a', 'b'],
            vec!['a', 'c'],
            vec!['b', 'a'],
        ];

        assert_eq!(
            visible_hint_labels(&labels, &[]),
            vec![
                (0, vec!['a']),
                (1, vec!['b']),
                (2, vec!['a', 'b']),
                (3, vec!['a', 'c']),
                (4, vec!['b', 'a']),
            ]
        );
        assert_eq!(
            visible_hint_labels(&labels, &['a']),
            vec![(0, vec![]), (2, vec!['b']), (3, vec!['c'])]
        );
        assert_eq!(visible_hint_labels(&labels, &['z']), Vec::new());
    }

    #[test]
    fn sort_dedup_hint_matches_orders_by_start_and_keeps_first_duplicate() {
        #[derive(Debug, Eq, PartialEq)]
        struct Match {
            id: &'static str,
            start: Pos,
        }

        let mut matches = vec![
            Match {
                id: "line1-col8",
                start: Pos::new(Line(1), Column(8)),
            },
            Match {
                id: "line0-col7",
                start: Pos::new(Line(0), Column(7)),
            },
            Match {
                id: "line0-col7-duplicate",
                start: Pos::new(Line(0), Column(7)),
            },
            Match {
                id: "line0-col3",
                start: Pos::new(Line(0), Column(3)),
            },
        ];

        sort_dedup_hint_matches_by_start(&mut matches, |hint_match| hint_match.start);

        assert_eq!(
            matches
                .into_iter()
                .map(|hint_match| hint_match.id)
                .collect::<Vec<_>>(),
            vec!["line0-col3", "line0-col7", "line1-col8"]
        );
    }

    #[test]
    fn terminal_file_link_token_at_extracts_token_under_column() {
        assert_eq!(
            terminal_file_link_token_at("open src/main.rs now", 8),
            Some(FileLinkToken {
                col_start: 5,
                col_end: 16,
                text: "src/main.rs".to_string(),
            })
        );
    }

    #[test]
    fn terminal_file_link_token_at_trims_trailing_punctuation_for_resolution() {
        assert_eq!(
            terminal_file_link_token_at("error: src/main.rs: done", 12),
            Some(FileLinkToken {
                col_start: 7,
                col_end: 19,
                text: "src/main.rs".to_string(),
            })
        );
        assert_eq!(
            terminal_file_link_token_at("see ./notes.md.", 6).map(|token| (
                token.col_start,
                token.col_end,
                token.text
            )),
            Some((4, 15, "./notes.md".to_string()))
        );
    }

    #[test]
    fn terminal_file_link_token_at_rejects_delimiters_and_out_of_bounds() {
        assert_eq!(terminal_file_link_token_at("src/main.rs", 99), None);
        assert_eq!(terminal_file_link_token_at("src/main.rs other", 11), None);
        assert_eq!(terminal_file_link_token_at("(src/main.rs)", 0), None);
    }

    #[test]
    fn visible_hyperlink_hint_matches_walks_osc8_spans() {
        let terminal = terminal_with_osc8(
            b"go \x1b]8;;https://example.com/path]\x07click\x1b]8;;\x07.",
        );

        assert_eq!(
            visible_hyperlink_hint_matches(&terminal, true),
            vec![HintTextMatch {
                text: "https://example.com/path".to_string(),
                start: pos(0, 3),
                end: pos(0, 7),
            }]
        );
        assert_eq!(
            visible_hyperlink_hint_matches(&terminal, false)[0].text,
            "https://example.com/path]"
        );
    }
}
