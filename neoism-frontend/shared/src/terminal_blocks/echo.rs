use neoism_terminal_core::crosswords::grid::row::Row;
use neoism_terminal_core::crosswords::square::Square;
use std::collections::BTreeMap;

use super::chrome::row_is_empty;
use super::chrome::row_text;
use super::command::{BlockStatusKind, CommandBlockSnapshot};

pub fn block_echo_map_for_window(
    raw_rows: &[Row<Square>],
    raw_sources: &[usize],
    snapshots: &[CommandBlockSnapshot],
) -> BTreeMap<usize, usize> {
    debug_assert_eq!(raw_sources.len(), raw_rows.len());

    let mut block_at_row: Vec<(usize, usize)> = snapshots
        .iter()
        .enumerate()
        .filter_map(|(idx, b)| b.output_start_row.map(|abs| (abs, idx)))
        .collect();
    block_at_row.sort_by_key(|(abs, _)| *abs);

    let mut block_emitted = vec![false; snapshots.len()];
    let row_texts = raw_rows.iter().map(row_text).collect::<Vec<_>>();
    let mut next_text_block =
        text_echo_start_index_hint(&row_texts, raw_sources, snapshots);
    let mut echo_rows = BTreeMap::new();

    for ((row, text), &abs) in raw_rows
        .iter()
        .zip(row_texts.iter())
        .zip(raw_sources.iter())
    {
        let block_idx = block_at_row
            .binary_search_by_key(&abs, |(a, _)| *a)
            .ok()
            .map(|i| block_at_row[i].1)
            .filter(|i| !block_emitted[*i])
            .filter(|i| abs_matched_text_still_looks_like_echo(row, text, &snapshots[*i]))
            .or_else(|| {
                next_text_matched_echo_block_text(
                    text,
                    snapshots,
                    &block_emitted,
                    &mut next_text_block,
                )
            });

        if let Some(block_idx) = block_idx {
            block_emitted[block_idx] = true;
            echo_rows.insert(abs, block_idx);
        }
    }

    echo_rows
}

pub fn abs_matched_text_still_looks_like_echo(
    row: &Row<Square>,
    text: &str,
    snapshot: &CommandBlockSnapshot,
) -> bool {
    text_matches_command_echo(text.trim(), snapshot.command.trim())
        || (matches!(snapshot.status, BlockStatusKind::Running) && row_is_empty(row))
}

pub fn text_echo_start_index_hint(
    row_texts: &[String],
    sources: &[usize],
    snapshots: &[CommandBlockSnapshot],
) -> usize {
    if snapshots.is_empty() {
        return 0;
    }
    let mut candidate_count = 0usize;
    let mut first_candidate_abs = None;
    for (text, &abs) in row_texts.iter().zip(sources.iter()) {
        if text_matches_any_command_echo(text.trim(), snapshots) {
            candidate_count += 1;
            first_candidate_abs.get_or_insert(abs);
        }
    }
    let Some(first_abs) = first_candidate_abs else {
        return snapshots
            .iter()
            .position(|snapshot| {
                snapshot
                    .output_start_row
                    .is_some_and(|start| start >= sources.first().copied().unwrap_or(0))
            })
            .unwrap_or(0);
    };
    let base = snapshots
        .iter()
        .position(|snapshot| {
            snapshot
                .output_start_row
                .is_some_and(|start| start >= first_abs)
        })
        .unwrap_or(snapshots.len());
    base.min(snapshots.len().saturating_sub(candidate_count.max(1)))
}

pub fn next_text_matched_echo_block_text(
    text: &str,
    snapshots: &[CommandBlockSnapshot],
    block_emitted: &[bool],
    next_text_block: &mut usize,
) -> Option<usize> {
    while *next_text_block < snapshots.len() && block_emitted[*next_text_block] {
        *next_text_block += 1;
    }

    let mut idx = *next_text_block;
    while idx < snapshots.len() {
        if !block_emitted[idx]
            && text_matches_command_echo(text.trim(), snapshots[idx].command.trim())
        {
            *next_text_block = idx + 1;
            return Some(idx);
        }
        idx += 1;
    }

    None
}

pub fn text_matches_any_command_echo(
    text: &str,
    snapshots: &[CommandBlockSnapshot],
) -> bool {
    !text.is_empty()
        && snapshots
            .iter()
            .any(|snapshot| text_matches_command_echo(text, snapshot.command.trim()))
}

pub fn text_matches_command_echo(text: &str, command: &str) -> bool {
    if command.is_empty() {
        return false;
    }
    text == command
        || text
            .strip_suffix(command)
            .is_some_and(|prefix| prefix.ends_with(char::is_whitespace))
}
