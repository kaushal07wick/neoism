use super::*;
use super::render::{
    cached_message_height, display_timeline_message, f32_measure_bucket, is_edit_tool_message,
};
use super::read_group::read_tool_group_at;

pub(crate) fn from_state_cache(
    cache: crate::panels::agent_pane::state::TimelineLayoutCache,
) -> TimelineLayoutCache<NeoismAgentMessage> {
    TimelineLayoutCache {
        epoch: cache.epoch,
        source_len: cache.source_len,
        width_bucket: cache.width_bucket,
        scale_bucket: cache.scale_bucket,
        gap_bucket: cache.gap_bucket,
        content_height: cache.content_height,
        pages: cache
            .pages
            .into_iter()
            .map(|page| TimelineLayoutPage {
                page_index: page.page_index,
                source_start: page.source_start,
                source_end: page.source_end,
                row_start: page.row_start,
                row_end: page.row_end,
                top: page.top,
                height: page.height,
                measured: page.measured,
            })
            .collect(),
        rows: cache
            .rows
            .into_iter()
            .map(|row| TimelineLayoutRow {
                source_index: row.source_index,
                source_end_index: row.source_end_index,
                top: row.top,
                height: row.height,
                display_text: row.display_text,
                display_message: row.display_message,
                markdown_blocks: row.markdown_blocks,
                tool_diff_sections: row.tool_diff_sections,
                is_edit_tool: row.is_edit_tool,
            })
            .collect(),
    }
}

pub(crate) fn into_state_cache(
    cache: TimelineLayoutCache<NeoismAgentMessage>,
) -> crate::panels::agent_pane::state::TimelineLayoutCache {
    crate::panels::agent_pane::state::TimelineLayoutCache {
        epoch: cache.epoch,
        source_len: cache.source_len,
        width_bucket: cache.width_bucket,
        scale_bucket: cache.scale_bucket,
        gap_bucket: cache.gap_bucket,
        content_height: cache.content_height,
        pages: cache
            .pages
            .into_iter()
            .map(
                |page| crate::panels::agent_pane::state::TimelineLayoutPage {
                    page_index: page.page_index,
                    source_start: page.source_start,
                    source_end: page.source_end,
                    row_start: page.row_start,
                    row_end: page.row_end,
                    top: page.top,
                    height: page.height,
                    measured: page.measured,
                },
            )
            .collect(),
        rows: cache
            .rows
            .into_iter()
            .map(|row| crate::panels::agent_pane::state::TimelineLayoutRow {
                source_index: row.source_index,
                source_end_index: row.source_end_index,
                top: row.top,
                height: row.height,
                display_text: row.display_text,
                display_message: row.display_message,
                markdown_blocks: row.markdown_blocks,
                tool_diff_sections: row.tool_diff_sections,
                is_edit_tool: row.is_edit_tool,
            })
            .collect(),
    }
}

pub(crate) fn visible_timeline_row_range<M>(
    rows: &[TimelineLayoutRow<M>],
    visible_top: f32,
    visible_bottom: f32,
) -> Range<usize> {
    if rows.is_empty() || visible_bottom < visible_top {
        return 0..0;
    }

    // Rows are laid out in ascending `top` order. Find the first row whose
    // bottom edge can intersect the registration band, then stop once row tops
    // are below it. This keeps ordinary scroll frames proportional to visible
    // cards instead of total history length while preserving the existing
    // offscreen registration margin used for hit-testing/selection.
    let start = rows.partition_point(|row| row.top + row.height < visible_top);
    let end = start + rows[start..].partition_point(|row| row.top <= visible_bottom);
    start..end
}

pub(crate) fn timeline_row_range_for_source_range<M>(
    rows: &[TimelineLayoutRow<M>],
    source_start: usize,
    source_end: usize,
) -> Range<usize> {
    if rows.is_empty() || source_end < source_start {
        return 0..0;
    }
    let start = rows.partition_point(|row| row.source_end_index < source_start);
    let end = start + rows[start..].partition_point(|row| row.source_index <= source_end);
    start..end
}

pub(crate) fn timeline_row_range_intersects_viewport<M>(
    rows: &[TimelineLayoutRow<M>],
    range: Range<usize>,
    visible_top: f32,
    visible_bottom: f32,
) -> bool {
    if range.is_empty() || visible_bottom < visible_top {
        return false;
    }
    rows[range].iter().any(|row| {
        let bottom = row.top + row.height;
        bottom >= visible_top && row.top <= visible_bottom
    })
}

pub(crate) fn union_timeline_row_ranges(a: Range<usize>, b: Range<usize>) -> Range<usize> {
    if a.is_empty() {
        return b;
    }
    if b.is_empty() {
        return a;
    }
    a.start.min(b.start)..a.end.max(b.end)
}

pub(crate) fn timeline_virtual_row_measurements<M>(
    rows: &[TimelineLayoutRow<M>],
    gap: f32,
) -> Vec<TimelineVirtualRowMeasurement> {
    rows.iter()
        .enumerate()
        .map(|(ix, row)| {
            let trailing_gap = if ix + 1 < rows.len() { gap } else { 0.0 };
            TimelineVirtualRowMeasurement {
                source_index: row.source_index,
                source_end_index: row.source_end_index,
                height: (row.height + trailing_gap).max(0.0),
                visual_line_count: (row.height / 20.0).ceil().max(1.0) as u32,
            }
        })
        .collect()
}

pub(crate) fn timeline_layout<P, D>(
    sugarloaf: &mut Sugarloaf,
    pane: &mut P,
    width: f32,
    theme: &IdeTheme,
    s: f32,
    gap: f32,
) -> (TimelineLayoutCache<P::Message>, bool)
where
    P: AgentTimelinePane,
    D: AgentTimelineDelegate<P>,
{
    let width_bucket = f32_measure_bucket(width);
    let scale_bucket = f32_measure_bucket(s);
    let gap_bucket = f32_measure_bucket(gap);
    let epoch = pane.timeline_layout_epoch();
    let mut dirty = pane.take_timeline_dirty_marks();
    if pane.any_tool_expand_animating() {
        mark_animating_tool_rows_dirty(pane, &mut dirty);
    }
    let source_len = pane.messages().len();
    // History pagination: messages were prepended at the front. Lay out only
    // the new prefix and shift the existing rows, instead of rebuilding the
    // whole transcript. This keeps each "load older" proportional to what was
    // added rather than to total loaded history — without it, every page load
    // re-measured and re-keyed every row already on screen, so pagination got
    // progressively slower with each page.
    if let Some(delta) = pane.take_timeline_prepend() {
        // Always pull the cache out: after a prepend every existing row's
        // source index has shifted, so the normal append-patch path below
        // would mis-target. Either we fold incrementally here, or we drop the
        // cache and fall through to a clean full rebuild.
        //
        // Crucially this runs even when there are dirty marks: paginating
        // while the agent streams (the common case) always has a dirty tail,
        // and gating the fold on "no dirty" forced a full O(total) rebuild on
        // every page — the slowdown that compounded with each load. Instead we
        // fold the prepend, then patch the (index-shifted) dirty tail.
        let reusable = pane.take_timeline_layout_cache().filter(|cache| {
            delta > 0
                && cache.epoch == epoch
                && cache.width_bucket == width_bucket
                && cache.scale_bucket == scale_bucket
                && cache.gap_bucket == gap_bucket
                && cache.source_len + delta == source_len
                && !cache.rows.is_empty()
        });
        if let Some(mut cache) = reusable {
            if prepend_timeline_layout::<P, D>(
                sugarloaf, pane, &mut cache, delta, width, theme, s, gap,
            ) {
                // The prepend shifted every existing row by `delta`; shift the
                // pending dirty indices to match, then re-lay just that region.
                if !dirty.ids.is_empty() || !dirty.indices.is_empty() {
                    let shifted = TimelineDirtyMarks {
                        ids: dirty.ids,
                        indices: dirty
                            .indices
                            .iter()
                            .map(|index| index + delta)
                            .collect(),
                    };
                    patch_timeline_layout::<P, D>(
                        sugarloaf, pane, &mut cache, shifted, width, theme, s, gap,
                    );
                }
                return (cache, true);
            }
        }
    }
    let cache = pane.take_timeline_layout_cache().filter(|cache| {
        cache.epoch == epoch
            && cache.width_bucket == width_bucket
            && cache.scale_bucket == scale_bucket
            && cache.gap_bucket == gap_bucket
            && cache.source_len <= source_len
            && (cache.source_len == 0 || !cache.rows.is_empty())
    });

    if let Some(mut cache) = cache {
        let needs_patch = cache.source_len != source_len
            || !dirty.ids.is_empty()
            || !dirty.indices.is_empty();
        if !needs_patch
            || patch_timeline_layout::<P, D>(
                sugarloaf, pane, &mut cache, dirty, width, theme, s, gap,
            )
        {
            if source_len == 0 || !cache.rows.is_empty() {
                return (cache, needs_patch);
            }
        }
    }

    (
        build_timeline_layout::<P, D>(
            sugarloaf,
            pane,
            width,
            theme,
            s,
            gap,
            epoch,
            width_bucket,
            scale_bucket,
            gap_bucket,
        ),
        true,
    )
}

fn mark_animating_tool_rows_dirty<P: AgentTimelinePane>(
    pane: &P,
    dirty: &mut TimelineDirtyMarks,
) {
    for (index, message) in pane.messages().iter().enumerate() {
        if pane.tool_expand_animating(message.id()) {
            dirty.indices.insert(index);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_timeline_layout<P, D>(
    sugarloaf: &mut Sugarloaf,
    pane: &mut P,
    width: f32,
    theme: &IdeTheme,
    s: f32,
    gap: f32,
    epoch: u64,
    width_bucket: i32,
    scale_bucket: i32,
    gap_bucket: i32,
) -> TimelineLayoutCache<P::Message>
where
    P: AgentTimelinePane,
    D: AgentTimelineDelegate<P>,
{
    let mut rows: Vec<TimelineLayoutRow<P::Message>> = Vec::new();
    let content_height = append_timeline_rows::<P, D>(
        sugarloaf, pane, width, theme, s, gap, 0, 0.0, false, &mut rows,
    );
    let pages = build_timeline_layout_pages(&rows, pane.messages().len());
    TimelineLayoutCache {
        epoch,
        source_len: pane.messages().len(),
        width_bucket,
        scale_bucket,
        gap_bucket,
        content_height,
        pages,
        rows,
    }
}

#[allow(clippy::too_many_arguments)]
fn patch_timeline_layout<P, D>(
    sugarloaf: &mut Sugarloaf,
    pane: &mut P,
    cache: &mut TimelineLayoutCache<P::Message>,
    dirty: TimelineDirtyMarks,
    width: f32,
    theme: &IdeTheme,
    s: f32,
    gap: f32,
) -> bool
where
    P: AgentTimelinePane,
    D: AgentTimelineDelegate<P>,
{
    let source_len = pane.messages().len();
    let Some(start_source) = dirty_start_source(pane, cache, &dirty) else {
        return cache.source_len == source_len;
    };
    let start_pos = cache
        .rows
        .iter()
        .position(|row| {
            row.source_index >= start_source || row.source_end_index >= start_source
        })
        .unwrap_or(cache.rows.len());
    let (scan_start, content_y, previous_visible_was_edit_tool) = if let Some(previous) =
        start_pos
            .checked_sub(1)
            .and_then(|index| cache.rows.get(index))
    {
        (
            previous.source_end_index.saturating_add(1),
            previous.top + previous.height,
            previous.is_edit_tool,
        )
    } else {
        (0, 0.0, false)
    };
    cache.rows.truncate(start_pos);
    cache.content_height = append_timeline_rows::<P, D>(
        sugarloaf,
        pane,
        width,
        theme,
        s,
        gap,
        scan_start,
        content_y,
        previous_visible_was_edit_tool,
        &mut cache.rows,
    );
    cache.source_len = source_len;
    cache.pages = build_timeline_layout_pages(&cache.rows, source_len);
    true
}

fn prepared_message_markdown_blocks<P>(
    sugarloaf: &mut Sugarloaf,
    pane: &P,
    message: &P::Message,
    width: f32,
    theme: &IdeTheme,
    s: f32,
) -> Option<Rc<Vec<AssistantMarkdownBlock>>>
where
    P: AgentTimelinePane,
{
    let text = message.text();
    if text.trim().is_empty() {
        return None;
    }
    let markdown_width = match message.kind() {
        AgentTimelineMessageKind::Assistant => {
            (width - 30.0 * s - ASSISTANT_TEXT_PAD_LEFT * s).max(80.0 * s)
        }
        AgentTimelineMessageKind::Reasoning | AgentTimelineMessageKind::Compaction => {
            (width - 48.0 * s).max(80.0 * s)
        }
        _ => return None,
    };
    Some(layout_assistant_markdown_cached(
        sugarloaf,
        pane,
        text,
        markdown_width,
        theme,
        s,
    ))
}

pub(crate) fn prepared_message_tool_diff_sections<M>(message: &M) -> Option<CachedToolDiffSections>
where
    M: AgentTimelineMessage,
{
    if message.kind() != AgentTimelineMessageKind::Tool {
        return None;
    }
    cached_edit_diff_sections_for_parts(
        message.id(),
        message.title(),
        message.text(),
        message.status(),
        message.tool(),
        message.detail(),
    )
}

/// Incrementally fold `delta` freshly-prepended messages into an existing
/// layout. Lays out the new prefix from source 0, then — as soon as the
/// running edit-tool state lines up with an existing cached row boundary —
/// reuses every row from there on by shifting its source index (`+delta`)
/// and its `top` (by the height the prefix added). Returns `false` if it
/// can't safely converge, leaving the caller to fall back to a full rebuild.
///
/// Correctness: an existing row's shape depends only on its (immutable)
/// message and the `previous_visible_was_edit_tool` flowing in. Once that
/// incoming state matches at a shared boundary, the remaining cached rows
/// are identical in shape and only their vertical offset changes — so the
/// shift is exact.
#[allow(clippy::too_many_arguments)]
fn prepend_timeline_layout<P, D>(
    sugarloaf: &mut Sugarloaf,
    pane: &mut P,
    cache: &mut TimelineLayoutCache<P::Message>,
    delta: usize,
    width: f32,
    theme: &IdeTheme,
    s: f32,
    gap: f32,
) -> bool
where
    P: AgentTimelinePane,
    D: AgentTimelineDelegate<P>,
{
    let source_len = pane.messages().len();
    if delta == 0 || cache.source_len + delta != source_len {
        return false;
    }

    // Map: old cached row at old source index `k` now lives at new index
    // `k + delta`. Find, for a given new source index, the cached row that
    // starts exactly there (cached rows are sorted ascending by source index).
    let cached_row_starting_at = |new_source_index: usize| -> Option<usize> {
        new_source_index.checked_sub(delta).and_then(|old_index| {
            cache
                .rows
                .binary_search_by(|row| row.source_index.cmp(&old_index))
                .ok()
        })
    };
    // The edit-tool state the cached layout had flowing into row `pos`.
    let cached_incoming_edit_tool = |pos: usize| -> bool {
        pos.checked_sub(1)
            .map(|prev| cache.rows[prev].is_edit_tool)
            .unwrap_or(false)
    };

    let mut new_rows: Vec<TimelineLayoutRow<P::Message>> = Vec::new();
    let mut content_y = 0.0_f32;
    let mut previous_visible_was_edit_tool = false;
    let mut source_index = 0usize;
    let mut converge: Option<(usize, f32)> = None;

    while source_index < source_len {
        // Once we reach previously-laid-out territory, try to converge: if the
        // running state matches a cached row boundary, every row from there on
        // is reusable as-is (only its offset changes).
        if source_index >= delta {
            if let Some(pos) = cached_row_starting_at(source_index) {
                if cached_incoming_edit_tool(pos) == previous_visible_was_edit_tool {
                    converge = Some((pos, content_y));
                    break;
                }
            }
        }

        let Some(item) =
            next_timeline_item::<P>(pane, source_index, previous_visible_was_edit_tool)
        else {
            source_index += 1;
            continue;
        };
        match item {
            NextTimelineItem::Group {
                source_end_exclusive,
                message: group_message,
            } => {
                let height = cached_message_height::<P, D>(
                    sugarloaf,
                    pane,
                    &group_message,
                    width,
                    theme,
                    s,
                );
                if height > 0.0 {
                    if !new_rows.is_empty() {
                        content_y += gap;
                    }
                    let markdown_blocks = prepared_message_markdown_blocks(
                        sugarloaf,
                        pane,
                        &group_message,
                        width,
                        theme,
                        s,
                    );
                    let tool_diff_sections =
                        prepared_message_tool_diff_sections(&group_message);
                    new_rows.push(TimelineLayoutRow {
                        source_index,
                        source_end_index: source_end_exclusive.saturating_sub(1),
                        top: content_y,
                        height,
                        display_text: None,
                        display_message: Some(group_message),
                        markdown_blocks,
                        tool_diff_sections,
                        is_edit_tool: false,
                    });
                    content_y += height;
                }
                previous_visible_was_edit_tool = false;
                source_index = source_end_exclusive;
            }
            NextTimelineItem::Message { message } => {
                let height = cached_message_height::<P, D>(
                    sugarloaf, pane, &message, width, theme, s,
                );
                if height <= 0.0 {
                    source_index += 1;
                    continue;
                }
                if !new_rows.is_empty() {
                    content_y += gap;
                }
                let is_edit_tool = is_edit_tool_message(&message);
                let markdown_blocks = prepared_message_markdown_blocks(
                    sugarloaf, pane, &message, width, theme, s,
                );
                let tool_diff_sections = prepared_message_tool_diff_sections(&message);
                new_rows.push(TimelineLayoutRow {
                    source_index,
                    source_end_index: source_index,
                    top: content_y,
                    height,
                    display_text: None,
                    display_message: Some(message),
                    markdown_blocks,
                    tool_diff_sections,
                    is_edit_tool,
                });
                previous_visible_was_edit_tool = is_edit_tool;
                content_y += height;
                source_index += 1;
            }
        }
    }

    if let Some((pos, prefix_end_y)) = converge {
        // Splice: new prefix rows, then the reused cached suffix, shifted.
        let gap_before_suffix = if new_rows.is_empty() { 0.0 } else { gap };
        let offset = (prefix_end_y + gap_before_suffix) - cache.rows[pos].top;
        let mut rows = new_rows;
        rows.reserve(cache.rows.len() - pos);
        for mut row in cache.rows.drain(pos..) {
            row.source_index += delta;
            row.source_end_index += delta;
            row.top += offset;
            rows.push(row);
        }
        cache.rows = rows;
    } else {
        // Never converged — `new_rows` is a full layout of the whole
        // transcript (equivalent to a rebuild), so adopt it wholesale.
        cache.rows = new_rows;
    }

    cache.content_height = cache
        .rows
        .last()
        .map(|row| row.top + row.height)
        .unwrap_or(0.0);
    cache.source_len = source_len;
    cache.pages = build_timeline_layout_pages(&cache.rows, source_len);
    true
}

fn dirty_start_source<P: AgentTimelinePane>(
    pane: &P,
    cache: &TimelineLayoutCache<P::Message>,
    dirty: &TimelineDirtyMarks,
) -> Option<usize> {
    let source_len = pane.messages().len();
    let mut start = if source_len > cache.source_len {
        cache.source_len.checked_sub(1).or(Some(cache.source_len))
    } else {
        None
    };
    for index in dirty
        .indices
        .iter()
        .copied()
        .filter(|index| *index < source_len)
    {
        start = Some(start.map_or(index, |current| current.min(index)));
    }
    for id in &dirty.ids {
        let Some(index) = pane
            .messages()
            .iter()
            .position(|message| message.id() == id)
        else {
            return Some(0);
        };
        start = Some(start.map_or(index, |current| current.min(index)));
    }
    start
}

#[allow(clippy::too_many_arguments)]
fn append_timeline_rows<P, D>(
    sugarloaf: &mut Sugarloaf,
    pane: &mut P,
    width: f32,
    theme: &IdeTheme,
    s: f32,
    gap: f32,
    start_index: usize,
    mut content_y: f32,
    mut previous_visible_was_edit_tool: bool,
    rows: &mut Vec<TimelineLayoutRow<P::Message>>,
) -> f32
where
    P: AgentTimelinePane,
    D: AgentTimelineDelegate<P>,
{
    let mut appended_any = false;
    let source_len = pane.messages().len();
    let mut source_index = start_index;
    while source_index < source_len {
        let Some(item) =
            next_timeline_item::<P>(pane, source_index, previous_visible_was_edit_tool)
        else {
            source_index += 1;
            continue;
        };
        match item {
            NextTimelineItem::Group {
                source_end_exclusive,
                message: group_message,
            } => {
                let height = cached_message_height::<P, D>(
                    sugarloaf,
                    pane,
                    &group_message,
                    width,
                    theme,
                    s,
                );
                if height > 0.0 {
                    if appended_any || !rows.is_empty() {
                        content_y += gap;
                    }
                    let markdown_blocks = prepared_message_markdown_blocks(
                        sugarloaf,
                        pane,
                        &group_message,
                        width,
                        theme,
                        s,
                    );
                    let tool_diff_sections =
                        prepared_message_tool_diff_sections(&group_message);
                    rows.push(TimelineLayoutRow {
                        source_index,
                        source_end_index: source_end_exclusive.saturating_sub(1),
                        top: content_y,
                        height,
                        display_text: None,
                        display_message: Some(group_message),
                        markdown_blocks,
                        tool_diff_sections,
                        is_edit_tool: false,
                    });
                    content_y += height;
                    appended_any = true;
                }
                previous_visible_was_edit_tool = false;
                source_index = source_end_exclusive;
            }
            NextTimelineItem::Message { message } => {
                let height = cached_message_height::<P, D>(
                    sugarloaf, pane, &message, width, theme, s,
                );
                if height <= 0.0 {
                    source_index += 1;
                    continue;
                }
                if appended_any || !rows.is_empty() {
                    content_y += gap;
                }
                let is_edit_tool = is_edit_tool_message(&message);
                let markdown_blocks = prepared_message_markdown_blocks(
                    sugarloaf, pane, &message, width, theme, s,
                );
                let tool_diff_sections = prepared_message_tool_diff_sections(&message);
                rows.push(TimelineLayoutRow {
                    source_index,
                    source_end_index: source_index,
                    top: content_y,
                    height,
                    display_text: None,
                    display_message: Some(message),
                    markdown_blocks,
                    tool_diff_sections,
                    is_edit_tool,
                });
                previous_visible_was_edit_tool = is_edit_tool;
                content_y += height;
                appended_any = true;
                source_index += 1;
            }
        }
    }
    content_y
}

enum NextTimelineItem<M> {
    Group {
        source_end_exclusive: usize,
        message: M,
    },
    Message {
        message: M,
    },
}

fn next_timeline_item<P>(
    pane: &P,
    source_index: usize,
    previous_visible_was_edit_tool: bool,
) -> Option<NextTimelineItem<P::Message>>
where
    P: AgentTimelinePane,
{
    let messages = pane.messages();
    if let Some((source_end_exclusive, group_message)) =
        read_tool_group_at(messages, source_index)
    {
        return Some(NextTimelineItem::Group {
            source_end_exclusive,
            message: group_message,
        });
    }
    let message = messages.get(source_index)?;
    display_timeline_message(message, previous_visible_was_edit_tool)
        .map(|message| NextTimelineItem::Message { message })
}

fn build_timeline_layout_pages<M>(
    rows: &[TimelineLayoutRow<M>],
    source_len: usize,
) -> Vec<TimelineLayoutPage> {
    if source_len == 0 {
        return Vec::new();
    }

    let page_count = source_len.div_ceil(TIMELINE_PAGE_SOURCE_LEN);
    let mut pages = Vec::with_capacity(page_count);
    let mut row_cursor = 0;
    for page_index in 0..page_count {
        let source_start = page_index * TIMELINE_PAGE_SOURCE_LEN;
        let source_end = ((page_index + 1) * TIMELINE_PAGE_SOURCE_LEN).min(source_len);
        while row_cursor < rows.len() && rows[row_cursor].source_end_index < source_start
        {
            row_cursor += 1;
        }
        let row_start = row_cursor;
        let mut row_end = row_start;
        while row_end < rows.len() && rows[row_end].source_index < source_end {
            row_end += 1;
        }
        let (top, height, measured) = if row_start < row_end {
            let top = rows[row_start].top;
            let bottom = rows[row_end - 1].top + rows[row_end - 1].height;
            (top, (bottom - top).max(0.0), true)
        } else {
            (0.0, 0.0, false)
        };
        pages.push(TimelineLayoutPage {
            page_index,
            source_start,
            source_end,
            row_start,
            row_end,
            top,
            height,
            measured,
        });
        row_cursor = row_end;
    }
    pages
}
