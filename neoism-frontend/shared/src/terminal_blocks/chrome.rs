use neoism_terminal_core::crosswords::grid::row::Row;
use neoism_terminal_core::crosswords::square::Square;
use neoism_terminal_core::crosswords::style::DEFAULT_STYLE_ID;
use std::collections::BTreeSet;

use super::command::CommandBlockSnapshot;
use super::echo::{
    abs_matched_text_still_looks_like_echo, block_echo_map_for_window,
    next_text_matched_echo_block_text, text_echo_start_index_hint,
};

/// Chrome takes TWO cell rows in the layout — META on top, COMMAND
/// below. Both are real stream cells so they scroll exactly like
/// any other PTY row through the GPU's residual-uniform pipeline,
/// no overlay tricks.
///
/// The important rule is that the renderer scrolls a composed
/// block-list row stream, not raw PTY rows plus late overlay rows.
/// A shell echo row has visual height 2 (META + COMMAND) and the
/// echo cells themselves are suppressed. Other PTY rows have visual
/// height 1. No neighboring output row is deleted to "balance" the
/// math; the scroll cursor steps through the composed stream.
pub const COMMAND_BLOCK_CHROME_ROWS: usize = 2;
pub const COMMAND_BLOCK_META_ROW: usize = 0;
pub const COMMAND_BLOCK_COMMAND_ROW: usize = 1;

/// Position of one block's header rows in the composed frame, in
/// final display-row coordinates. `chrome_row_count` is the original
/// chrome height (always `COMMAND_BLOCK_CHROME_ROWS`); the visible
/// span may be shorter when the chrome is partly clipped by the
/// viewport edge.
#[derive(Debug, Clone, Copy)]
pub struct BlockHeaderSpan {
    pub block_idx: usize,
    pub start_display_row: isize,
    pub end_display_row: isize,
    pub first_chrome_row: usize,
    pub chrome_row_count: usize,
}

/// Output of `compose_block_chrome`: the visible row stream that the
/// GPU emits for a terminal pane that uses Warp-style command blocks.
/// Chrome rows appear as empty `Row<Square>` placeholders with `None`
/// in `source_row_indices`; the chrome decoration is painted on top
/// via the sugarloaf overlay pass at `block_header_spans` positions.
#[derive(Debug, Clone)]
pub struct BlockChromeFrame {
    pub rows: Vec<Row<Square>>,
    pub source_row_indices: Vec<Option<usize>>,
    pub block_header_spans: Vec<BlockHeaderSpan>,
}

/// A composed viewport plus the hidden edge rows used by the GPU's
/// fractional terminal scroll buffer. The edge rows come from the same
/// virtual block-list stream as `frame.rows`, which is the invariant
/// required for pixel scrolling to behave like normal terminal text.
#[derive(Debug, Clone)]
pub struct BlockChromeWindow {
    pub frame: BlockChromeFrame,
    pub snapshot_above: Option<Row<Square>>,
    pub snapshot_below: Option<Row<Square>>,
    pub top_abs: Option<usize>,
    pub top_chrome_row: usize,
    pub echo_rows: BTreeSet<usize>,
}

#[derive(Debug, Clone)]
pub enum ComposedBlockRow {
    Pty {
        abs: usize,
        row: Row<Square>,
    },
    Chrome {
        abs: usize,
        block_idx: usize,
        chrome_row: usize,
    },
}

impl ComposedBlockRow {
    fn abs(&self) -> usize {
        match self {
            ComposedBlockRow::Pty { abs, .. } => *abs,
            ComposedBlockRow::Chrome { abs, .. } => *abs,
        }
    }

    fn row(&self, columns: usize) -> Row<Square> {
        match self {
            ComposedBlockRow::Pty { row, .. } => row.clone(),
            ComposedBlockRow::Chrome { .. } => Row::new(columns),
        }
    }

    fn source(&self) -> Option<usize> {
        match self {
            ComposedBlockRow::Pty { abs, .. } => Some(*abs),
            ComposedBlockRow::Chrome { .. } => None,
        }
    }

    fn matches_anchor(&self, anchor_abs: usize, anchor_chrome_row: usize) -> bool {
        match self {
            ComposedBlockRow::Pty { abs, .. } => *abs == anchor_abs,
            ComposedBlockRow::Chrome {
                abs, chrome_row, ..
            } => *abs == anchor_abs && *chrome_row == anchor_chrome_row,
        }
    }

    fn anchor(&self) -> (usize, usize) {
        match self {
            ComposedBlockRow::Pty { abs, .. } => (*abs, 0),
            ComposedBlockRow::Chrome {
                abs, chrome_row, ..
            } => (*abs, *chrome_row),
        }
    }
}

/// Build a Warp-style composed row window around `anchor_abs`.
///
/// `raw_rows/raw_sources` should cover enough PTY history before and
/// after the anchor to fill the viewport plus one hidden edge row on
/// each side. The function converts command echo rows into two real
/// chrome rows, slices the composed stream at `anchor_abs` /
/// `anchor_chrome_row`, and returns both visible rows and the hidden
/// above/below rows from that same composed stream.
pub fn compose_block_chrome_window(
    raw_rows: Vec<Row<Square>>,
    raw_sources: Vec<usize>,
    snapshots: &[CommandBlockSnapshot],
    viewport_rows: usize,
    anchor_abs: usize,
    anchor_chrome_row: usize,
) -> BlockChromeWindow {
    let viewport_rows = viewport_rows.max(1);
    let columns = raw_rows
        .first()
        .map(|row| row.inner.len())
        .unwrap_or(1)
        .max(1);
    debug_assert_eq!(raw_sources.len(), raw_rows.len());

    let echo_rows = block_echo_map_for_window(&raw_rows, &raw_sources, snapshots);
    let mut composed = Vec::with_capacity(raw_rows.len());

    for (row, abs) in raw_rows.into_iter().zip(raw_sources.into_iter()) {
        if let Some(&block_idx) = echo_rows.get(&abs) {
            for chrome_row in 0..COMMAND_BLOCK_CHROME_ROWS {
                composed.push(ComposedBlockRow::Chrome {
                    abs,
                    block_idx,
                    chrome_row,
                });
            }
        } else {
            composed.push(ComposedBlockRow::Pty { abs, row });
        }
    }

    let anchor_chrome_row =
        anchor_chrome_row.min(COMMAND_BLOCK_CHROME_ROWS.saturating_sub(1));
    let top_idx = composed
        .iter()
        .position(|row| row.matches_anchor(anchor_abs, anchor_chrome_row))
        .or_else(|| composed.iter().position(|row| row.abs() >= anchor_abs))
        .unwrap_or(0);

    block_chrome_window_from_composed(
        composed,
        columns,
        viewport_rows,
        top_idx,
        0,
        echo_rows.keys().copied().collect(),
    )
}

/// Build a composed row window pinned to the bottom of the supplied
/// raw PTY window. Used for the live terminal state (`display_offset ==
/// 0`): new command block rows should grow upward from just above the
/// composer, not enter from the top of the viewport.
pub fn compose_block_chrome_window_pinned_bottom(
    raw_rows: Vec<Row<Square>>,
    raw_sources: Vec<usize>,
    snapshots: &[CommandBlockSnapshot],
    viewport_rows: usize,
) -> BlockChromeWindow {
    let viewport_rows = viewport_rows.max(1);
    let columns = raw_rows
        .first()
        .map(|row| row.inner.len())
        .unwrap_or(1)
        .max(1);
    debug_assert_eq!(raw_sources.len(), raw_rows.len());

    let echo_rows = block_echo_map_for_window(&raw_rows, &raw_sources, snapshots);
    let mut composed = Vec::with_capacity(raw_rows.len());
    for (row, abs) in raw_rows.into_iter().zip(raw_sources.into_iter()) {
        if let Some(&block_idx) = echo_rows.get(&abs) {
            for chrome_row in 0..COMMAND_BLOCK_CHROME_ROWS {
                composed.push(ComposedBlockRow::Chrome {
                    abs,
                    block_idx,
                    chrome_row,
                });
            }
        } else {
            composed.push(ComposedBlockRow::Pty { abs, row });
        }
    }

    let top_padding = viewport_rows.saturating_sub(composed.len());
    let top_idx = if top_padding > 0 {
        0
    } else {
        composed.len().saturating_sub(viewport_rows)
    };
    block_chrome_window_from_composed(
        composed,
        columns,
        viewport_rows,
        top_idx,
        top_padding,
        echo_rows.keys().copied().collect(),
    )
}

fn block_chrome_window_from_composed(
    composed: Vec<ComposedBlockRow>,
    columns: usize,
    viewport_rows: usize,
    top_idx: usize,
    top_padding: usize,
    echo_rows: BTreeSet<usize>,
) -> BlockChromeWindow {
    let top_idx = top_idx.min(composed.len());
    let mut rows = Vec::with_capacity(viewport_rows);
    let mut sources = Vec::with_capacity(viewport_rows);
    let top_padding = top_padding.min(viewport_rows);
    for _ in 0..top_padding {
        rows.push(Row::new(columns));
        sources.push(None);
    }
    let composed_visible_rows = viewport_rows - top_padding;
    for idx in top_idx..top_idx + composed_visible_rows {
        if let Some(row) = composed.get(idx) {
            rows.push(row.row(columns));
            sources.push(row.source());
        } else {
            rows.push(Row::new(columns));
            sources.push(None);
        }
    }

    let snapshot_above = if top_padding > 0 {
        None
    } else {
        top_idx
            .checked_sub(1)
            .and_then(|idx| composed.get(idx))
            .map(|row| row.row(columns))
    };
    let snapshot_below = composed
        .get(top_idx + composed_visible_rows)
        .map(|row| row.row(columns));

    let (top_abs, top_chrome_row) = composed
        .get(top_idx)
        .map(|row| row.anchor())
        .map_or((None, 0), |(abs, chrome_row)| (Some(abs), chrome_row));

    let mut spans: Vec<BlockHeaderSpan> = Vec::new();
    let span_start = top_idx.saturating_sub(1);
    let span_end = (top_idx + composed_visible_rows + 1).min(composed.len());
    for idx in span_start..span_end {
        let ComposedBlockRow::Chrome {
            block_idx,
            chrome_row,
            ..
        } = &composed[idx]
        else {
            continue;
        };
        let block_idx = *block_idx;
        let chrome_row = *chrome_row;
        let display_row = top_padding as isize + idx as isize - top_idx as isize;
        if let Some(last) = spans.last_mut() {
            let next_chrome_row = last.first_chrome_row
                + (last.end_display_row - last.start_display_row) as usize;
            if last.block_idx == block_idx
                && last.end_display_row == display_row
                && next_chrome_row == chrome_row
            {
                last.end_display_row += 1;
                continue;
            }
        }
        spans.push(BlockHeaderSpan {
            block_idx,
            start_display_row: display_row,
            end_display_row: display_row + 1,
            first_chrome_row: chrome_row,
            chrome_row_count: COMMAND_BLOCK_CHROME_ROWS,
        });
    }

    BlockChromeWindow {
        frame: BlockChromeFrame {
            rows,
            source_row_indices: sources,
            block_header_spans: spans,
        },
        snapshot_above,
        snapshot_below,
        top_abs,
        top_chrome_row,
        echo_rows,
    }
}

/// Build the visible row stream for a Warp-style terminal pane. Takes
/// raw PTY visible rows + their absolute source indices and the
/// current command-block snapshots. Inserts blank chrome rows at each
/// block boundary visible in the window so output naturally shifts
/// down. The result has length ≤ `viewport_rows`; when the PTY
/// hasn't filled the area below the prompt the stream is bottom-
/// aligned (Warp's `PinnedToBottom`) so output sits just above the
/// off-grid composer instead of hugging the top of the pane.
#[allow(dead_code)]
pub fn compose_block_chrome(
    visible_rows: Vec<Row<Square>>,
    visible_sources: Vec<usize>,
    snapshots: &[CommandBlockSnapshot],
    viewport_rows: usize,
) -> BlockChromeFrame {
    let viewport_rows = viewport_rows.max(1);
    let columns = visible_rows
        .first()
        .map(|row| row.inner.len())
        .unwrap_or(1)
        .max(1);
    debug_assert_eq!(visible_sources.len(), visible_rows.len());

    // Build a lookup from PTY abs row -> block index for blocks
    // whose header position is in the visible window. Only one block
    // can claim a given abs row (output_start_row is unique per
    // block), so a small Vec<(abs, block_idx)> beats a HashMap here.
    let mut block_at_row: Vec<(usize, usize)> = snapshots
        .iter()
        .enumerate()
        .filter_map(|(idx, b)| b.output_start_row.map(|abs| (abs, idx)))
        .collect();
    block_at_row.sort_by_key(|(abs, _)| *abs);

    // Single-pass walk: copy raw rows into the new stream. At each
    // block's "$ <cmd>" echo row, emit a chrome row and drop the
    // echo. Net change is exactly 0 per block — that's what makes
    // pixel-perfect scroll smooth across every block boundary
    // (stream length never varies as a function of which blocks
    // are visible).
    //
    // We identify the echo two ways:
    //   1. Abs match: row's abs equals the block's stored
    //      `output_start_row`. Fast path when the PTY hasn't
    //      reflowed.
    //   2. Text match (fallback): row's text equals the next
    //      un-emitted block's command. Catches the post-resize
    //      case (Ctrl+/-) where the PTY reflows and stored abs
    //      values shift but cell content stays — without this
    //      the echo would stay in the stream and render twice
    //      (once as itself, once again under the chrome label).
    let mut block_emitted = vec![false; snapshots.len()];
    let row_texts = visible_rows.iter().map(row_text).collect::<Vec<_>>();
    let mut next_text_block =
        text_echo_start_index_hint(&row_texts, &visible_sources, snapshots);
    let mut new_rows: Vec<Row<Square>> = Vec::with_capacity(visible_rows.len());
    let mut sources_opt: Vec<Option<usize>> = Vec::with_capacity(visible_sources.len());
    let mut spans: Vec<BlockHeaderSpan> = Vec::new();
    for ((row, text), abs) in visible_rows
        .into_iter()
        .zip(row_texts.iter())
        .zip(visible_sources.into_iter())
    {
        let block_idx = block_at_row
            .binary_search_by_key(&abs, |(a, _)| *a)
            .ok()
            .map(|i| block_at_row[i].1)
            .filter(|i| !block_emitted[*i])
            .filter(|i| {
                abs_matched_text_still_looks_like_echo(&row, text, &snapshots[*i])
            })
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
            let span_start = new_rows.len();
            for _ in 0..COMMAND_BLOCK_CHROME_ROWS {
                new_rows.push(Row::new(columns));
                sources_opt.push(None);
            }
            spans.push(BlockHeaderSpan {
                block_idx,
                start_display_row: span_start as isize,
                end_display_row: (span_start + COMMAND_BLOCK_CHROME_ROWS) as isize,
                first_chrome_row: 0,
                chrome_row_count: COMMAND_BLOCK_CHROME_ROWS,
            });
            // Drop the echo row — chrome above paints the same label.
            continue;
        }
        new_rows.push(row);
        sources_opt.push(Some(abs));
    }

    // Fit to viewport. Drop trailing PTY blanks first (genuine empty
    // PTY output below the prompt). Chrome placeholder rows have a
    // `None` source and must NOT be counted — dropping those would
    // chop the latest block's header off the bottom and pop it back
    // in next commit.
    //
    // We keep the cap at exactly viewport_rows. Extending to +1 to
    // smooth the first commit (META lands at the same visible row
    // before and after the boundary commit) traded one jump for
    // many: every subsequent commit transitioning from the chrome-
    // at-end stream back to a normal stream popped the next output
    // row in instead of letting it slide via the GPU's
    // snapshot_below buffer slot. Holding the cap at viewport_rows
    // means snapshot_below stays at slot (viewport_rows + 1) where
    // the residual offset can pull it into the last visible row
    // smoothly — and the only cost is a single 1-cell jump on the
    // very first commit when a block's chrome appears at the
    // bottom edge.
    let max_stream = viewport_rows;
    if new_rows.len() > max_stream {
        let overflow = new_rows.len() - max_stream;
        let trailing = trailing_pty_empty_count(&new_rows, &sources_opt);
        if trailing >= overflow {
            let mut drops = overflow;
            while drops > 0 {
                let last_idx = new_rows.len() - 1;
                if sources_opt[last_idx].is_some() && row_is_empty(&new_rows[last_idx]) {
                    new_rows.pop();
                    sources_opt.pop();
                    drops -= 1;
                } else {
                    break;
                }
            }
        } else {
            new_rows.drain(0..overflow);
            sources_opt.drain(0..overflow);
            for span in &mut spans {
                span.start_display_row -= overflow as isize;
                span.end_display_row -= overflow as isize;
            }
            spans.retain(|s| s.end_display_row > 0);
        }
    }

    // Bottom-align — Warp's PinnedToBottom. When trailing PTY rows
    // are still blank (sparse output above the prompt) move them to
    // the head so live content sits just above the composer instead
    // of hugging the top of the pane. Same `None`-source skip applies
    // here: chrome rows are NOT padding and must stay where they are.
    let trailing = trailing_pty_empty_count(&new_rows, &sources_opt);
    if trailing > 0 && trailing < new_rows.len() {
        let new_len = new_rows.len() - trailing;
        new_rows.truncate(new_len);
        sources_opt.truncate(new_len);
        for _ in 0..trailing {
            new_rows.insert(0, Row::new(columns));
            sources_opt.insert(0, None);
        }
        for span in &mut spans {
            span.start_display_row += trailing as isize;
            span.end_display_row += trailing as isize;
        }
    }

    // Pad to viewport_rows with blanks at the head. Without this,
    // when the row stream is shorter than the budget (e.g. PTY hasn't
    // scrolled enough yet to fill the area below the prompt) the GPU
    // emits len rows starting at panel top and leaves a gap above
    // the composer at the bottom.
    while new_rows.len() < viewport_rows {
        new_rows.insert(0, Row::new(columns));
        sources_opt.insert(0, None);
        for span in &mut spans {
            span.start_display_row += 1;
            span.end_display_row += 1;
        }
    }

    BlockChromeFrame {
        rows: new_rows,
        source_row_indices: sources_opt,
        block_header_spans: spans,
    }
}

/// Compute the position spans for each command-block header that has
/// landed inside the current viewport. Warp does NOT pin chrome to
/// the viewport top — once the header has scrolled above the
/// visible window it stays scrolled out of view, so this skips
/// blocks whose `output_start_row` is above `visible_sources[0]` or
/// past the last visible source.
#[allow(dead_code)]
pub fn block_header_spans_for_sources(
    snapshots: &[CommandBlockSnapshot],
    visible_sources: &[usize],
    viewport_rows: usize,
) -> Vec<BlockHeaderSpan> {
    let viewport_rows = viewport_rows.max(1);
    let Some(&viewport_start) = visible_sources.first() else {
        return Vec::new();
    };
    let Some(&last_visible) = visible_sources.last() else {
        return Vec::new();
    };
    let viewport_end = last_visible.saturating_add(1);
    let mut spans = Vec::new();
    for (block_idx, block) in snapshots.iter().enumerate() {
        let Some(block_start) = block.output_start_row else {
            continue;
        };
        if block_start < viewport_start || block_start >= viewport_end {
            continue;
        }
        let start_display_row = lower_bound(visible_sources, block_start);
        if start_display_row >= viewport_rows {
            continue;
        }
        let end_display_row =
            (start_display_row + COMMAND_BLOCK_CHROME_ROWS).min(viewport_rows);
        if start_display_row < end_display_row {
            spans.push(BlockHeaderSpan {
                block_idx,
                start_display_row: start_display_row as isize,
                end_display_row: end_display_row as isize,
                first_chrome_row: 0,
                chrome_row_count: COMMAND_BLOCK_CHROME_ROWS,
            });
        }
    }
    spans
}

#[allow(dead_code)]
pub fn trailing_empty_count(rows: &[Row<Square>]) -> usize {
    rows.iter().rev().take_while(|r| row_is_empty(r)).count()
}

/// Count trailing rows whose source is `Some(...)` (real PTY rows)
/// AND the cell row is empty. Skips chrome / pad placeholders
/// (`None` source) so they aren't accidentally dropped from the
/// bottom of the stream.
#[allow(dead_code)]
pub fn trailing_pty_empty_count(
    rows: &[Row<Square>],
    sources: &[Option<usize>],
) -> usize {
    rows.iter()
        .zip(sources.iter())
        .rev()
        .take_while(|(r, s)| s.is_some() && row_is_empty(r))
        .count()
}

pub fn row_text(row: &Row<Square>) -> String {
    row.inner
        .iter()
        .map(|cell| match cell.c() {
            '\0' => ' ',
            ch => ch,
        })
        .collect()
}

pub fn row_is_empty(row: &Row<Square>) -> bool {
    row.inner.iter().all(|cell| {
        if cell.is_bg_only() || cell.has_graphics() {
            return false;
        }
        let c = cell.c();
        (c == ' ' || c == '\0' || c == '\t')
            && cell.style_id() == DEFAULT_STYLE_ID
            && cell.extras_id().is_none()
    })
}

#[allow(dead_code)]
pub fn lower_bound(values: &[usize], needle: usize) -> usize {
    let mut left = 0usize;
    let mut right = values.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if values[mid] < needle {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}
