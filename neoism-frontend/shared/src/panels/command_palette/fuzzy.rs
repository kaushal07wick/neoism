// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Fuzzy scoring + small numeric/text helpers shared across the panel.
//!
//! Geometry / easing / text-truncation primitives live in
//! `chrome::primitives` and are re-exported below so existing
//! `super::fuzzy::*` imports keep compiling.

pub(crate) use crate::animation::{ease_out_back, ease_out_cubic};
pub(crate) use crate::primitives::{snap_to_device_px, truncate_to_fit};

use super::SCROLL_OFF_ROWS;

/// Fuzzy match: checks if all query chars appear in order in the target.
/// Returns a score (higher = better match), or None if no match.
pub(crate) fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    if query_lower.is_empty() {
        return Some(0);
    }

    let mut qi = 0;
    let mut score: i32 = 0;
    let mut prev_match = false;
    let mut first_match_pos = None;

    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_lower.len() && tc == query_lower[qi] {
            if first_match_pos.is_none() {
                first_match_pos = Some(ti);
            }
            // Consecutive match bonus
            if prev_match {
                score += 5;
            }
            // Word boundary bonus (start of string or after space/punctuation)
            if ti == 0 || !target_lower[ti - 1].is_alphanumeric() {
                score += 10;
            }
            prev_match = true;
            qi += 1;
        } else {
            prev_match = false;
        }
    }

    if qi < query_lower.len() {
        return None; // Not all query chars matched
    }

    // Bonus for matching near the start
    if let Some(pos) = first_match_pos {
        score += (20_i32).saturating_sub(pos as i32);
    }

    Some(score)
}

pub(crate) fn scrolloff_for(visible_rows: usize) -> usize {
    if visible_rows <= 2 {
        return 0;
    }
    SCROLL_OFF_ROWS.min(visible_rows.saturating_sub(1) / 2)
}
