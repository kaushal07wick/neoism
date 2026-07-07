//! Incremental in-buffer `/` search for the markdown pane.
//!
//! The nvim-backed code editor gets its `/` incsearch from the managed
//! nvim (`rio.search` lua). Markdown panes run neoism's *own* vim engine
//! and renderer, so this is the native equivalent: opening `/` snapshots
//! the view, each keystroke re-scans the buffer and jumps the cursor to
//! the nearest match (wrapping, like nvim), every occurrence lights up in
//! the renderer, Enter commits the pattern to the `n`/`N` engine, and Esc
//! restores the pre-search cursor + scroll.
//!
//! The transient input lives in [`VimState::incsearch`] so no `MarkdownPane`
//! constructor has to change; the committed pattern still lands in
//! [`VimState::search`] where `n`/`N` already read it.

use super::*;

impl MarkdownPane {
    /// `true` while the `/` (or `?`) search prompt is capturing input.
    pub fn search_active(&self) -> bool {
        self.vim.incsearch.is_some()
    }

    /// The pattern typed so far, if the prompt is open.
    pub fn search_query(&self) -> Option<&str> {
        self.vim.incsearch.as_ref().map(|s| s.query.as_str())
    }

    /// Whether the open session searches backward (`?`).
    pub fn search_reverse(&self) -> bool {
        self.vim
            .incsearch
            .as_ref()
            .map(|s| s.reverse)
            .unwrap_or(false)
    }

    /// The cmdline label drawn at the bottom of the pane (`/pattern`).
    pub fn search_prompt(&self) -> Option<String> {
        let s = self.vim.incsearch.as_ref()?;
        let sigil = if s.reverse { '?' } else { '/' };
        Some(format!("{sigil}{}", s.query))
    }

    /// `(current, total)` for the `[cur/total]` count. `current` is
    /// 1-based; `(0, total)` when the pattern matches nothing yet.
    pub fn search_count(&self) -> Option<(usize, usize)> {
        let s = self.vim.incsearch.as_ref()?;
        let total = s.matches.len();
        let cur = if total == 0 || s.current == usize::MAX {
            0
        } else {
            s.current.min(total - 1) + 1
        };
        Some((cur, total))
    }

    /// Open the `/` (forward) or `?` (backward) incremental search,
    /// snapshotting the current view so a cancel restores it.
    pub fn search_open(&mut self, reverse: bool) {
        self.vim.clear_pending();
        self.vim.incsearch = Some(MarkdownIncSearch {
            query: String::new(),
            reverse,
            origin_line: self.cursor_line,
            origin_col: self.cursor_col,
            origin_scroll_y: self.scroll_y,
            origin_target_scroll_y: self.target_scroll_y,
            matches: Vec::new(),
            current: usize::MAX,
        });
    }

    /// Append a typed character to the pattern and re-run the search.
    pub fn search_push_char(&mut self, ch: char) {
        if let Some(s) = self.vim.incsearch.as_mut() {
            s.query.push(ch);
        } else {
            return;
        }
        self.search_recompute_and_focus();
    }

    /// Backspace one character. Emptying the pattern returns the cursor
    /// to the origin (nvim incsearch behaviour).
    pub fn search_backspace(&mut self) {
        let emptied = match self.vim.incsearch.as_mut() {
            Some(s) => {
                s.query.pop();
                s.query.is_empty()
            }
            None => return,
        };
        if emptied {
            if let Some(s) = self.vim.incsearch.as_mut() {
                s.matches.clear();
                s.current = usize::MAX;
            }
            self.search_restore_origin_cursor();
        } else {
            self.search_recompute_and_focus();
        }
    }

    /// Enter: keep the cursor on the focused match, hand the pattern to
    /// the `n`/`N` engine, and close the prompt.
    pub fn search_commit(&mut self) {
        let Some(s) = self.vim.incsearch.take() else {
            return;
        };
        if !s.query.is_empty() && !s.matches.is_empty() {
            self.vim.search = Some(VimSearch {
                pattern: s.query,
                forward: !s.reverse,
                whole_word: false,
            });
        }
    }

    /// Esc: restore the pre-search cursor + scroll and close the prompt.
    pub fn search_cancel(&mut self) {
        if self.vim.incsearch.is_none() {
            return;
        }
        self.search_restore_origin_view();
        self.vim.incsearch = None;
    }

    /// Jump to the next/prev committed match (`n`/`N`). Mirrors the
    /// nvim-editor `n`/`N`; used by the web mini-handler, which does not
    /// route through the full `VimAction` engine.
    pub fn search_repeat(&mut self, reverse: bool) -> bool {
        let Some(search) = self.vim.search.clone() else {
            return false;
        };
        let forward = search.forward != reverse;
        let pos = self.cursor_position();
        let next = if forward {
            vim_search_forward(&self.lines, pos, &search.pattern, search.whole_word)
        } else {
            vim_search_backward(&self.lines, pos, &search.pattern, search.whole_word)
        };
        let Some(next) = next else {
            return false;
        };
        self.cursor_line = next.line.min(self.lines.len().saturating_sub(1));
        self.cursor_col = next
            .col
            .min(self.lines.get(self.cursor_line).map(String::len).unwrap_or(0));
        self.follow_cursor = true;
        true
    }

    /// Match ranges on `line_ix` for the renderer: `(start_byte, end_byte,
    /// is_current)`. The focused match reports `true` so it can paint
    /// brighter (the current-match accent vs. the dimmer all-match wash).
    pub fn search_matches_for_line(&self, line_ix: usize) -> Vec<(usize, usize, bool)> {
        let Some(s) = self.vim.incsearch.as_ref() else {
            return Vec::new();
        };
        if s.query.is_empty() {
            return Vec::new();
        }
        let qlen = s.query.len();
        s.matches
            .iter()
            .enumerate()
            .filter(|(_, (li, _))| *li == line_ix)
            .map(|(ix, (_, col))| (*col, col + qlen, ix == s.current))
            .collect()
    }

    fn search_restore_origin_cursor(&mut self) {
        let Some(s) = self.vim.incsearch.as_ref() else {
            return;
        };
        let line = s.origin_line.min(self.lines.len().saturating_sub(1));
        let col = s
            .origin_col
            .min(self.lines.get(line).map(String::len).unwrap_or(0));
        self.cursor_line = line;
        self.cursor_col = col;
        self.follow_cursor = true;
    }

    fn search_restore_origin_view(&mut self) {
        let Some(s) = self.vim.incsearch.as_ref() else {
            return;
        };
        let line = s.origin_line.min(self.lines.len().saturating_sub(1));
        let col = s
            .origin_col
            .min(self.lines.get(line).map(String::len).unwrap_or(0));
        let scroll_y = s.origin_scroll_y;
        let target_scroll_y = s.origin_target_scroll_y;
        self.cursor_line = line;
        self.cursor_col = col;
        self.scroll_y = scroll_y;
        self.target_scroll_y = target_scroll_y;
        // View already restored — don't let the follow-cursor reveal drag
        // the scroll somewhere else this frame.
        self.follow_cursor = false;
    }

    fn search_recompute_and_focus(&mut self) {
        let (query, oline, ocol, reverse) = {
            let Some(s) = self.vim.incsearch.as_ref() else {
                return;
            };
            (s.query.clone(), s.origin_line, s.origin_col, s.reverse)
        };
        // Case-sensitive, non-overlapping substring scan in file order —
        // matches the `*`/`n` engine so the live highlight, the committed
        // jump, and `n`/`N` all agree. `match_indices` keeps byte offsets
        // on char boundaries.
        let mut matches: Vec<(usize, usize)> = Vec::new();
        if !query.is_empty() {
            'outer: for (li, line) in self.lines.iter().enumerate() {
                for (col, _) in line.match_indices(query.as_str()) {
                    matches.push((li, col));
                    if matches.len() >= 5000 {
                        break 'outer;
                    }
                }
            }
        }
        let current = if matches.is_empty() {
            usize::MAX
        } else if reverse {
            nearest_before(&matches, oline, ocol)
        } else {
            nearest_after(&matches, oline, ocol)
        };
        if let Some(s) = self.vim.incsearch.as_mut() {
            s.matches = matches;
            s.current = current;
        }
        if current == usize::MAX {
            // No match for the current pattern — hold at the origin so the
            // view doesn't wander while the user keeps typing.
            self.search_restore_origin_cursor();
            return;
        }
        let (line, col) = self
            .vim
            .incsearch
            .as_ref()
            .map(|s| s.matches[s.current])
            .unwrap_or((oline, ocol));
        self.cursor_line = line.min(self.lines.len().saturating_sub(1));
        self.cursor_col = col.min(self.lines.get(self.cursor_line).map(String::len).unwrap_or(0));
        self.follow_cursor = true;
    }
}

/// Index of the first match at/after `(oline, ocol)`, wrapping to the
/// first match when every occurrence sits before the cursor.
fn nearest_after(matches: &[(usize, usize)], oline: usize, ocol: usize) -> usize {
    for (ix, (l, c)) in matches.iter().enumerate() {
        if *l > oline || (*l == oline && *c >= ocol) {
            return ix;
        }
    }
    0
}

/// Index of the last match at/before `(oline, ocol)`, wrapping to the
/// last match when every occurrence sits after the cursor.
fn nearest_before(matches: &[(usize, usize)], oline: usize, ocol: usize) -> usize {
    let mut best = matches.len().saturating_sub(1);
    for (ix, (l, c)) in matches.iter().enumerate() {
        if *l < oline || (*l == oline && *c <= ocol) {
            best = ix;
        } else {
            break;
        }
    }
    best
}
