use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use sugarloaf::text::DrawOpts;
use sugarloaf::Sugarloaf;

use crate::editor::neodraw::{render_scene, Camera, Vec2};
use crate::panels::agent_pane::state::NeoismAgentPane;

use super::code_block::{
    diff_line_kind, digit_count, render_code_line_background, render_code_line_text,
    syntax_lang, warm_code_lines_render_cache,
};
use super::draw::{
    draw_rect_clipped, draw_rounded_rect_clipped, draw_text_clipped,
    draw_top_rounded_rect_clipped, measure_text_cached, opts_with_clip,
};
use super::{ORDER_PANEL, ORDER_TEXT};
use crate::primitives::ide_theme::IdeTheme;
use crate::widgets::markdown as md;
use crate::widgets::mermaid::{
    measure_mermaid_diagram, mermaid_scene, parse_mermaid_diagram, MermaidDiagram,
};
use crate::widgets::stock_card::{
    measure_stock_card, parse_stock_card, render_stock_card, StockCardSpec,
};

mod inline_style;
mod mermaid;
use inline_style::{
    draw_hover_underline, parsed_markdown_inline_line, plain_token_color, rgba_from_u8,
};
use mermaid::render_mermaid_block;

const TABLE_CELL_LINE_H: f32 = 17.0;
const TABLE_ROW_PAD_Y: f32 = 12.0;
const TABLE_BLOCK_PAD_Y: f32 = 14.0;
/// Left pad (in unscaled px) applied to list-item text. The bullet marker
/// itself is no longer drawn, but its text keeps this indent so list lines
/// align with the surrounding chat content rather than going flush-left.
const BULLET_TEXT_INDENT: f32 = 18.0;
pub const COPY_LINK_PREFIX: &str = "neoism-copy://";
const COPY_REF_LINK_PREFIX: &str = "neoism-copy-ref://";
pub const MERMAID_TOGGLE_LINK_PREFIX: &str = "neoism-mermaid-toggle://";
const MARKDOWN_CODE_HEADER_H: f32 = 30.0;
const MARKDOWN_CODE_BODY_TOP_PAD: f32 = 10.0;
const MARKDOWN_CODE_BODY_BOTTOM_PAD: f32 = 10.0;
const MARKDOWN_CODE_LINE_H: f32 = 18.0;
const INLINE_LINE_CACHE_LIMIT: usize = 8192;
const COPY_SOURCE_CACHE_LIMIT: usize = 1024;

thread_local! {
    static INLINE_LINE_CACHE: RefCell<InlineLineCache> =
        RefCell::new(InlineLineCache::new());
    static COPY_SOURCE_CACHE: RefCell<CopySourceCache> =
        RefCell::new(CopySourceCache::new());
}

#[derive(Clone, Copy, Debug)]
enum PlainTokenColor {
    Accent,
    Blue,
    Cyan,
    Magenta,
    Yellow,
    SynType,
    SynString,
    Green,
    Red,
}

#[derive(Clone, Copy, Debug)]
struct PlainTokenStyle {
    color: PlainTokenColor,
    bold: bool,
}

#[derive(Clone, Debug)]
enum MarkdownInlineSegment {
    Text(String),
    Bold(String),
    Strike(String),
    Code {
        text: String,
        target: Option<String>,
    },
    MarkdownLink {
        label: String,
        target: Option<String>,
    },
    PlainToken {
        text: String,
        target: Option<String>,
        style: Option<PlainTokenStyle>,
    },
}

struct InlineLineCache {
    values: HashMap<String, Rc<Vec<MarkdownInlineSegment>>>,
    order: VecDeque<String>,
}

impl InlineLineCache {
    fn new() -> Self {
        Self {
            values: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, line: &str) -> Option<Rc<Vec<MarkdownInlineSegment>>> {
        self.values.get(line).cloned()
    }

    fn insert(&mut self, line: String, segments: Rc<Vec<MarkdownInlineSegment>>) {
        if self.values.contains_key(&line) {
            self.values.insert(line, segments);
            return;
        }
        self.order.push_back(line.clone());
        self.values.insert(line, segments);
        while self.order.len() > INLINE_LINE_CACHE_LIMIT {
            if let Some(old) = self.order.pop_front() {
                self.values.remove(&old);
            }
        }
    }
}

struct CopySourceCache {
    values: HashMap<String, Rc<Vec<String>>>,
    order: VecDeque<String>,
}

impl CopySourceCache {
    fn new() -> Self {
        Self {
            values: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, target: &str) -> Option<Rc<Vec<String>>> {
        self.values.get(target).cloned()
    }

    fn insert(&mut self, target: &str, lines: Rc<Vec<String>>) {
        if self.values.contains_key(target) {
            self.values.insert(target.to_string(), lines);
            return;
        }
        self.order.push_back(target.to_string());
        self.values.insert(target.to_string(), lines);
        while self.order.len() > COPY_SOURCE_CACHE_LIMIT {
            if let Some(old) = self.order.pop_front() {
                self.values.remove(&old);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum AssistantMarkdownBlock {
    Paragraph(Vec<String>),
    Heading {
        level: usize,
        lines: Vec<String>,
    },
    Bullet(Vec<String>),
    Quote(Vec<String>),
    Table(Vec<Vec<String>>),
    Code {
        lang: String,
        lines: Rc<Vec<String>>,
        copy_target: String,
    },
    Mermaid {
        source: String,
        lines: Vec<String>,
        diagram: Option<MermaidDiagram>,
        key: u64,
        copy_target: String,
    },
    Stock(StockCardSpec),
    Blank,
}

pub trait AgentMarkdownPane {
    fn cached_markdown_blocks_for(
        &self,
        text: &str,
        width: f32,
        scale: f32,
    ) -> Option<std::rc::Rc<Vec<AssistantMarkdownBlock>>>;

    fn store_markdown_blocks_for(
        &self,
        text: &str,
        width: f32,
        scale: f32,
        blocks: std::rc::Rc<Vec<AssistantMarkdownBlock>>,
    );

    fn register_selectable_line(&mut self, text: &str, rect: [f32; 4]) -> usize;
    fn selectable_line_highlight(&self, index: usize) -> Option<(f32, f32)>;
    fn register_link_hit_rect(&mut self, target: String, rect: [f32; 4]);
    fn link_hovered(&self, target: &str) -> bool;
    fn mermaid_raw_mode(&self, key: u64) -> bool;
    fn suppress_markdown_interactions(&self) -> bool {
        false
    }
}

impl AgentMarkdownPane for NeoismAgentPane {
    fn cached_markdown_blocks_for(
        &self,
        text: &str,
        width: f32,
        scale: f32,
    ) -> Option<std::rc::Rc<Vec<AssistantMarkdownBlock>>> {
        let key = NeoismAgentPane::markdown_blocks_key(text, width, scale);
        NeoismAgentPane::cached_markdown_blocks(self, &key)
    }

    fn store_markdown_blocks_for(
        &self,
        text: &str,
        width: f32,
        scale: f32,
        blocks: std::rc::Rc<Vec<AssistantMarkdownBlock>>,
    ) {
        let key = NeoismAgentPane::markdown_blocks_key(text, width, scale);
        NeoismAgentPane::store_markdown_blocks(self, key, blocks);
    }

    fn register_selectable_line(&mut self, text: &str, rect: [f32; 4]) -> usize {
        NeoismAgentPane::register_selectable_line(self, text, rect)
    }

    fn selectable_line_highlight(&self, index: usize) -> Option<(f32, f32)> {
        NeoismAgentPane::selectable_line_highlight(self, index)
    }

    fn register_link_hit_rect(&mut self, target: String, rect: [f32; 4]) {
        NeoismAgentPane::register_link_hit_rect(self, target, rect);
    }

    fn link_hovered(&self, target: &str) -> bool {
        NeoismAgentPane::link_hovered(self, target)
    }

    fn mermaid_raw_mode(&self, key: u64) -> bool {
        NeoismAgentPane::mermaid_raw_mode(self, key)
    }

    fn suppress_markdown_interactions(&self) -> bool {
        NeoismAgentPane::suppress_markdown_interactions(self)
    }
}

fn table_column_count(rows: &[Vec<String>]) -> usize {
    rows.iter().map(Vec::len).max().unwrap_or(1).max(1)
}

fn table_cell_text_width(table_w: f32, cols: usize, s: f32) -> f32 {
    let col_w = table_w / cols.max(1) as f32;
    (col_w - 28.0 * s).max(24.0 * s)
}

fn table_row_height_for_lines(lines: usize, s: f32) -> f32 {
    TABLE_ROW_PAD_Y * 2.0 * s + lines.max(1) as f32 * TABLE_CELL_LINE_H * s
}

fn wrap_table_cells(
    sugarloaf: &mut Sugarloaf,
    cells: Vec<String>,
    cell_w: f32,
    opts: &DrawOpts,
) -> Vec<String> {
    cells
        .into_iter()
        .map(|cell| {
            cell.lines()
                .flat_map(|line| {
                    md::wrap_words_measured(sugarloaf, line.trim(), cell_w, opts)
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

fn flush_layout_table(
    sugarloaf: &mut Sugarloaf,
    blocks: &mut Vec<AssistantMarkdownBlock>,
    rows: &mut Vec<Vec<String>>,
    width: f32,
    s: f32,
    opts: &DrawOpts,
) {
    if rows.is_empty() {
        return;
    }
    let cols = table_column_count(rows);
    let cell_w = table_cell_text_width(width, cols, s);
    let rows = std::mem::take(rows)
        .into_iter()
        .map(|row| wrap_table_cells(sugarloaf, row, cell_w, opts))
        .collect();
    blocks.push(AssistantMarkdownBlock::Table(rows));
}

/// Cached wrapper around [`layout_assistant_markdown`]. Memoises the
/// parsed + wrapped block list per `(text, width, scale)` on the pane.
/// Render and measurement share the cache; a stable history of N
/// messages costs O(visible) lookups per frame instead of O(visible)
/// markdown reparses.
/// Wrap a paragraph into pixel-bounded lines without ever splitting an
/// inline markdown atom (`**bold**`, `` `code` ``, `[label](target)`)
/// across line boundaries. `wrap_words_simple` only sees whitespace; if a
/// bold span contains a space and the wrap point lands inside it, the
/// downstream inline renderer (`draw_markdown_inline_line`) sees two
/// half-spans on adjacent lines and falls back to raw `**` text — a
/// visible "lost markdown" bug. This tokenizer treats each inline atom
/// as one indivisible unit.
fn wrap_inline_aware(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    width: f32,
    opts: &DrawOpts,
) -> Vec<String> {
    let tokens = inline_atoms(text);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for tok in tokens {
        let candidate = if current.is_empty() {
            tok.to_string()
        } else {
            format!("{current} {tok}")
        };
        if current.is_empty() || measure_text_cached(sugarloaf, &candidate, opts) <= width
        {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = tok.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Split `text` into atomic tokens for wrapping. Each token is either a
/// whitespace-bounded run, or a complete inline-markdown span. Spans:
/// - `**bold**`
/// - `` `code` ``
/// - `[label](target)`
/// If an opener isn't closed inside `text`, the token degrades to a
/// plain whitespace-bounded word so we never hang on a half-open span.
fn inline_atoms(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            break;
        }
        rest = trimmed;
        if let Some(after) = rest.strip_prefix("**") {
            if let Some(end_rel) = after.find("**") {
                let total = 2 + end_rel + 2;
                out.push(&rest[..total]);
                rest = &rest[total..];
                continue;
            }
        }
        if rest.starts_with('`') {
            if let Some(end_rel) = rest[1..].find('`') {
                let total = 1 + end_rel + 1;
                out.push(&rest[..total]);
                rest = &rest[total..];
                continue;
            }
        }
        if rest.starts_with('[') {
            if let Some(label_end) = rest.find(']') {
                if rest[label_end + 1..].starts_with('(') {
                    if let Some(target_end_rel) = rest[label_end + 2..].find(')') {
                        let total = label_end + 2 + target_end_rel + 1;
                        out.push(&rest[..total]);
                        rest = &rest[total..];
                        continue;
                    }
                }
            }
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    out
}

pub fn layout_assistant_markdown_cached<P: AgentMarkdownPane>(
    sugarloaf: &mut Sugarloaf,
    pane: &P,
    text: &str,
    width: f32,
    theme: &IdeTheme,
    s: f32,
) -> std::rc::Rc<Vec<AssistantMarkdownBlock>> {
    if let Some(hit) = pane.cached_markdown_blocks_for(text, width, s) {
        return hit;
    }
    super::derivations::bump_markdown_layout();
    let blocks =
        std::rc::Rc::new(layout_assistant_markdown(sugarloaf, text, width, theme, s));
    warm_markdown_code_blocks(blocks.as_slice());
    pane.store_markdown_blocks_for(text, width, s, blocks.clone());
    blocks
}

fn warm_markdown_code_blocks(blocks: &[AssistantMarkdownBlock]) {
    for block in blocks {
        if let AssistantMarkdownBlock::Code { lang, lines, .. } = block {
            warm_code_lines_render_cache(lines.as_slice(), lang);
        }
    }
}

pub fn layout_assistant_markdown(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    width: f32,
    theme: &IdeTheme,
    s: f32,
) -> Vec<AssistantMarkdownBlock> {
    let paragraph_opts = DrawOpts {
        font_size: 13.5 * s,
        color: theme.u8(theme.fg),
        ..DrawOpts::default()
    };
    let heading_opts = DrawOpts {
        font_size: 16.0 * s,
        color: theme.u8(theme.fg),
        bold: true,
        ..DrawOpts::default()
    };
    let mut blocks = Vec::new();
    let mut code: Option<(String, Vec<String>)> = None;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    for raw in text.lines() {
        let trimmed = raw.trim();
        if let Some(info) = md::fence_info(trimmed) {
            if let Some((lang, lines)) = code.take() {
                blocks.push(markdown_code_or_stock_block(lang, lines));
            } else {
                flush_layout_table(
                    sugarloaf,
                    &mut blocks,
                    &mut table_rows,
                    width,
                    s,
                    &paragraph_opts,
                );
                code = Some((info.to_string(), Vec::new()));
            }
            continue;
        }

        if let Some((_, lines)) = code.as_mut() {
            lines.push(raw.to_string());
            continue;
        }

        for raw in expand_inline_bullets(raw) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                flush_layout_table(
                    sugarloaf,
                    &mut blocks,
                    &mut table_rows,
                    width,
                    s,
                    &paragraph_opts,
                );
                if !matches!(blocks.last(), Some(AssistantMarkdownBlock::Blank)) {
                    blocks.push(AssistantMarkdownBlock::Blank);
                }
                continue;
            }

            if let Some(cells) = md::parse_table_row_trimmed(&raw) {
                if !md::is_table_separator_trimmed(&cells) {
                    table_rows.push(cells);
                }
                continue;
            }

            flush_layout_table(
                sugarloaf,
                &mut blocks,
                &mut table_rows,
                width,
                s,
                &paragraph_opts,
            );

            if let Some((level, heading)) = markdown_heading(&raw) {
                blocks.push(AssistantMarkdownBlock::Heading {
                    level,
                    lines: wrap_inline_aware(sugarloaf, heading, width, &heading_opts),
                });
            } else if let Some(bullet) = markdown_bullet(&raw) {
                blocks.push(AssistantMarkdownBlock::Bullet(wrap_inline_aware(
                    sugarloaf,
                    bullet,
                    (width - 28.0 * s).max(40.0 * s),
                    &paragraph_opts,
                )));
            } else if let Some(quote) = markdown_quote(&raw) {
                blocks.push(AssistantMarkdownBlock::Quote(wrap_inline_aware(
                    sugarloaf,
                    quote,
                    (width - 24.0 * s).max(40.0 * s),
                    &paragraph_opts,
                )));
            } else {
                blocks.push(AssistantMarkdownBlock::Paragraph(wrap_inline_aware(
                    sugarloaf,
                    trimmed,
                    width,
                    &paragraph_opts,
                )));
            }
        }
    }

    if let Some((lang, lines)) = code.take() {
        blocks.push(markdown_code_or_stock_block(lang, lines));
    }

    flush_layout_table(
        sugarloaf,
        &mut blocks,
        &mut table_rows,
        width,
        s,
        &paragraph_opts,
    );

    blocks
}

fn markdown_code_or_stock_block(
    lang: String,
    lines: Vec<String>,
) -> AssistantMarkdownBlock {
    if lang.trim().eq_ignore_ascii_case("mermaid") {
        let source = join_markdown_lines(&lines);
        let copy_target = format!("{COPY_LINK_PREFIX}{}", escape_copy_target(&source));
        return AssistantMarkdownBlock::Mermaid {
            diagram: parse_mermaid_diagram(&source),
            key: stable_hash(&source),
            source,
            lines,
            copy_target,
        };
    }
    if lang.trim().eq_ignore_ascii_case("stock") {
        let source = join_markdown_lines(&lines);
        if let Ok(spec) = parse_stock_card(&source) {
            return AssistantMarkdownBlock::Stock(spec);
        }
    }
    let copy_target = copy_ref_target_for_lines(&lines);
    AssistantMarkdownBlock::Code {
        lang,
        lines: Rc::new(lines),
        copy_target,
    }
}

fn join_markdown_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let capacity =
        lines.iter().map(String::len).sum::<usize>() + lines.len().saturating_sub(1);
    let mut output = String::with_capacity(capacity);
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(line);
    }
    output
}

/// Per-line height of a heading. The single source of truth shared by
/// [`markdown_block_height`] and the heading draw path so a heading occupies
/// exactly the space its card reserves.
fn heading_line_height(level: usize, s: f32) -> f32 {
    if level <= 2 {
        24.0 * s
    } else {
        22.0 * s
    }
}

/// Height of a single laid-out markdown block, excluding the 6*s inter-block
/// gap. This is the ONE place block heights are defined: both
/// [`measure_markdown_blocks`] (which sizes the message card) and
/// [`render_markdown_blocks`] (which advances the draw cursor) call it, so the
/// rendered content always fills exactly the measured card — no drift, no
/// gaps, no overflow.
pub fn markdown_block_height<P: AgentMarkdownPane>(
    block: &AssistantMarkdownBlock,
    width: f32,
    pane: &P,
    s: f32,
) -> f32 {
    match block {
        AssistantMarkdownBlock::Paragraph(lines) => lines.len().max(1) as f32 * 19.0 * s,
        AssistantMarkdownBlock::Heading { level, lines } => {
            4.0 * s + lines.len().max(1) as f32 * heading_line_height(*level, s)
        }
        AssistantMarkdownBlock::Bullet(lines) => lines.len().max(1) as f32 * 19.0 * s,
        AssistantMarkdownBlock::Quote(lines) => {
            4.0 * s + lines.len().max(1) as f32 * 19.0 * s
        }
        AssistantMarkdownBlock::Table(rows) => measure_laid_out_table_height(rows, s),
        AssistantMarkdownBlock::Code { lines, .. } => {
            let line_count = lines.len().max(1) as f32;
            (MARKDOWN_CODE_HEADER_H
                + MARKDOWN_CODE_BODY_TOP_PAD
                + MARKDOWN_CODE_BODY_BOTTOM_PAD
                + line_count * MARKDOWN_CODE_LINE_H)
                * s
        }
        AssistantMarkdownBlock::Mermaid {
            lines,
            diagram,
            key,
            ..
        } => {
            if pane.mermaid_raw_mode(*key) || diagram.is_none() {
                36.0 * s + lines.len().max(1) as f32 * 18.0 * s
            } else {
                36.0 * s
                    + measure_mermaid_diagram(
                        diagram.as_ref().expect("checked above"),
                        (width - 30.0 * s).max(260.0 * s),
                        s,
                    )
                    .height
            }
        }
        AssistantMarkdownBlock::Stock(spec) => measure_stock_card(spec, 0.0, s),
        AssistantMarkdownBlock::Blank => 8.0 * s,
    }
}

pub fn measure_markdown_blocks<P: AgentMarkdownPane>(
    blocks: &[AssistantMarkdownBlock],
    width: f32,
    pane: &P,
    s: f32,
) -> f32 {
    let mut height = 8.0 * s;
    for block in blocks {
        height += markdown_block_height(block, width, pane, s) + 6.0 * s;
    }
    height.max(22.0 * s)
}

#[allow(clippy::too_many_arguments)]
pub fn render_markdown_blocks<P: AgentMarkdownPane>(
    sugarloaf: &mut Sugarloaf,
    blocks: &[AssistantMarkdownBlock],
    x: f32,
    y: f32,
    w: f32,
    max_h: f32,
    pane: &mut P,
    theme: &IdeTheme,
    s: f32,
    body_muted: bool,
    marker_color: u32,
    show_leading_marker: bool,
    now_seconds: f32,
    mouse: Option<(f32, f32)>,
    viewport_clip: [f32; 4],
    occlusion_rects: &[[f32; 4]],
) {
    let mut cursor_y = y + 6.0 * s;
    // Cull blocks outside the viewport so a huge message only pays text
    // shaping for what's actually on screen, while still advancing the cursor
    // by each block's exact height — so positions match the measured card and
    // nothing drifts, overflows, or leaves a gap. `max_h` bounds the card; the
    // clip band (plus a little overscan for selection) bounds the viewport.
    let clip_top = viewport_clip[1];
    let clip_bottom = viewport_clip[1] + viewport_clip[3];
    let overscan = 240.0 * s;
    let bottom = (y + max_h).min(clip_bottom + overscan);
    let skip_above = clip_top - overscan;
    // Only offset content when we actually draw the leading status marker.
    let text_x = if show_leading_marker { x + 22.0 * s } else { x };
    let body_color = if body_muted { theme.white } else { theme.fg };
    // Reasoning blocks render in italic so the "inner monologue" reads
    // distinctly from the assistant's final answer. `body_muted` is the
    // reasoning signal — render_assistant_text passes false.
    let italic = body_muted;
    let body_text_color = if body_muted {
        theme.u8_alpha(theme.white, 0.6)
    } else {
        theme.u8(theme.fg)
    };
    let Some(opts) = opts_with_clip(
        DrawOpts {
            font_size: 13.5 * s,
            color: body_text_color,
            italic,
            ..DrawOpts::default()
        },
        viewport_clip,
    ) else {
        return;
    };
    let Some(muted_opts) = opts_with_clip(
        DrawOpts {
            font_size: 13.5 * s,
            color: theme.u8(theme.muted),
            italic,
            ..DrawOpts::default()
        },
        viewport_clip,
    ) else {
        return;
    };
    let suppress_interactions = pane.suppress_markdown_interactions();

    if show_leading_marker {
        // Center the marker dot against the visual middle of the first text
        // line. Paragraph rows start at `cursor_y = y + 6*s` with a 16*s
        // font; nudging the dot down a touch lines it up with the glyph
        // x-height instead of sitting above the cap-line.
        let marker_size = 6.0 * s;
        let marker_y = y + 6.0 * s + (16.0 * s - marker_size) * 0.5 + 1.0 * s;
        draw_rounded_rect_clipped(
            sugarloaf,
            [x + 4.0 * s, marker_y, marker_size, marker_size],
            theme.f32(marker_color),
            marker_size * 0.5,
            ORDER_TEXT,
            viewport_clip,
        );
    }

    for block in blocks {
        let block_h = markdown_block_height(block, w, pane, s);
        let block_top = cursor_y;
        let next_cursor = block_top + block_h + 6.0 * s;
        if block_top > bottom {
            break;
        }
        if block_top + block_h < skip_above {
            cursor_y = next_cursor;
            continue;
        }
        match block {
            AssistantMarkdownBlock::Paragraph(lines) => {
                let line_h = 19.0 * s;
                let (start_ix, end_ix) =
                    visible_line_range(cursor_y, line_h, lines.len(), viewport_clip);
                let mut line_y = cursor_y + start_ix as f32 * line_h;
                for line in &lines[start_ix..end_ix] {
                    draw_markdown_inline_line(
                        sugarloaf,
                        pane,
                        text_x,
                        line_y,
                        line,
                        &opts,
                        theme,
                        suppress_interactions,
                        viewport_clip,
                        occlusion_rects,
                    );
                    line_y += line_h;
                }
            }
            AssistantMarkdownBlock::Heading { level, lines } => {
                let font_size = match level {
                    1 => 21.0 * s,
                    2 => 18.0 * s,
                    _ => 16.0 * s,
                };
                let Some(heading_opts) = opts_with_clip(
                    DrawOpts {
                        font_size,
                        color: theme.u8(body_color),
                        bold: true,
                        ..DrawOpts::default()
                    },
                    viewport_clip,
                ) else {
                    return;
                };
                let line_top = cursor_y + 4.0 * s;
                let line_h = heading_line_height(*level, s);
                let (start_ix, end_ix) =
                    visible_line_range(line_top, line_h, lines.len(), viewport_clip);
                let mut line_y = line_top + start_ix as f32 * line_h;
                for line in &lines[start_ix..end_ix] {
                    draw_markdown_inline_line(
                        sugarloaf,
                        pane,
                        text_x,
                        line_y,
                        line,
                        &heading_opts,
                        theme,
                        suppress_interactions,
                        viewport_clip,
                        occlusion_rects,
                    );
                    line_y += line_h;
                }
            }
            AssistantMarkdownBlock::Bullet(lines) => {
                // The leading "-" / dot marker is intentionally NOT drawn —
                // the chat renders list items without a visible bullet. But
                // the text still gets the same `BULLET_TEXT_INDENT` left pad
                // the marker used to occupy, so list lines stay aligned with
                // the surrounding chat content (tool rows, paragraphs) instead
                // of slumping flush-left.
                let line_h = 19.0 * s;
                let (start_ix, end_ix) =
                    visible_line_range(cursor_y, line_h, lines.len(), viewport_clip);
                let mut line_y = cursor_y + start_ix as f32 * line_h;
                for line in &lines[start_ix..end_ix] {
                    draw_markdown_inline_line(
                        sugarloaf,
                        pane,
                        text_x + BULLET_TEXT_INDENT * s,
                        line_y,
                        line,
                        &opts,
                        theme,
                        suppress_interactions,
                        viewport_clip,
                        occlusion_rects,
                    );
                    line_y += line_h;
                }
            }
            AssistantMarkdownBlock::Quote(lines) => {
                let quote_top = cursor_y - 2.0 * s;
                let quote_h = lines.len().max(1) as f32 * 21.0 * s + 4.0 * s;
                draw_rect_clipped(
                    sugarloaf,
                    [text_x, quote_top, 2.0 * s, quote_h],
                    theme.f32(theme.border),
                    ORDER_TEXT,
                    viewport_clip,
                );
                let line_h = 19.0 * s;
                let (start_ix, end_ix) =
                    visible_line_range(cursor_y, line_h, lines.len(), viewport_clip);
                let mut line_y = cursor_y + start_ix as f32 * line_h;
                for line in &lines[start_ix..end_ix] {
                    draw_markdown_inline_line(
                        sugarloaf,
                        pane,
                        text_x + 14.0 * s,
                        line_y,
                        line,
                        &muted_opts,
                        theme,
                        suppress_interactions,
                        viewport_clip,
                        occlusion_rects,
                    );
                    line_y += line_h;
                }
            }
            AssistantMarkdownBlock::Code {
                lang,
                lines,
                copy_target,
            } => {
                render_markdown_code_block(
                    sugarloaf,
                    pane,
                    text_x,
                    cursor_y,
                    (w - 30.0 * s).max(80.0 * s),
                    block_h,
                    lang,
                    lines,
                    copy_target,
                    theme,
                    s,
                    body_muted,
                    suppress_interactions,
                    viewport_clip,
                    occlusion_rects,
                );
            }
            AssistantMarkdownBlock::Table(rows) => {
                render_markdown_table(
                    sugarloaf,
                    pane,
                    text_x,
                    cursor_y,
                    (w - 30.0 * s).max(80.0 * s),
                    block_h,
                    rows,
                    theme,
                    s,
                    suppress_interactions,
                    viewport_clip,
                    occlusion_rects,
                );
            }
            AssistantMarkdownBlock::Mermaid {
                source: _,
                lines,
                diagram,
                key,
                copy_target,
            } => {
                render_mermaid_block(
                    sugarloaf,
                    pane,
                    lines,
                    diagram.as_ref(),
                    *key,
                    copy_target,
                    text_x,
                    cursor_y,
                    (w - 30.0 * s).max(260.0 * s),
                    block_h,
                    theme,
                    s,
                    suppress_interactions,
                    viewport_clip,
                    occlusion_rects,
                );
            }
            AssistantMarkdownBlock::Stock(spec) => {
                render_stock_card(
                    sugarloaf,
                    spec,
                    text_x,
                    cursor_y,
                    (w - 30.0 * s).max(260.0 * s),
                    block_h,
                    theme,
                    s,
                    now_seconds,
                    mouse,
                    viewport_clip,
                    occlusion_rects,
                    super::DEPTH,
                    ORDER_PANEL + 2,
                );
            }
            AssistantMarkdownBlock::Blank => {}
        }
        // Snap to the block's exact bottom (+ inter-block gap) regardless of
        // any rounding inside the arm above, so render stays locked to
        // `measure_markdown_blocks`.
        cursor_y = next_cursor;
    }
}

fn visible_line_range(
    first_line_y: f32,
    line_h: f32,
    line_count: usize,
    viewport_clip: [f32; 4],
) -> (usize, usize) {
    if line_count == 0 || line_h <= 0.0 {
        return (0, 0);
    }

    let clip_top = viewport_clip[1];
    let clip_bottom = viewport_clip[1] + viewport_clip[3];
    let start_ix = ((clip_top - first_line_y - line_h) / line_h)
        .floor()
        .max(0.0) as usize;
    let end_ix = ((clip_bottom - first_line_y + line_h) / line_h)
        .ceil()
        .max(0.0) as usize;

    (start_ix.min(line_count), end_ix.min(line_count))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_markdown_code_block(
    sugarloaf: &mut Sugarloaf,
    pane: &mut impl AgentMarkdownPane,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    lang: &str,
    lines: &Rc<Vec<String>>,
    copy_target: &str,
    theme: &IdeTheme,
    s: f32,
    body_muted: bool,
    suppress_interactions: bool,
    viewport_clip: [f32; 4],
    occlusion_rects: &[[f32; 4]],
) {
    if h <= 0.0 {
        return;
    }
    let header_h = MARKDOWN_CODE_HEADER_H * s;
    let body_top = y + header_h;
    let first_line_y = body_top + MARKDOWN_CODE_BODY_TOP_PAD * s;
    let line_h = MARKDOWN_CODE_LINE_H * s;
    let clip_top = viewport_clip[1];
    let clip_bottom = viewport_clip[1] + viewport_clip[3];
    let start_ix = ((clip_top - first_line_y - line_h) / line_h)
        .floor()
        .max(0.0) as usize;
    let end_ix = ((clip_bottom - first_line_y + line_h) / line_h)
        .ceil()
        .max(0.0) as usize;
    let line_count = lines.len().max(1);
    let start_ix = start_ix.min(line_count);
    let end_ix = end_ix.min(line_count);
    let has_visible_text = start_ix < end_ix;
    if !has_visible_text
        && y + h > viewport_clip[1]
        && y < viewport_clip[1] + viewport_clip[3]
    {
        return;
    }
    let radius = 10.0 * s;
    let border_w = 1.0_f32.max(s);
    draw_rounded_rect_clipped(
        sugarloaf,
        [x, y, w, h],
        theme.f32(theme.border),
        radius,
        ORDER_PANEL,
        viewport_clip,
    );
    draw_rounded_rect_clipped(
        sugarloaf,
        [
            x + border_w,
            y + border_w,
            (w - 2.0 * border_w).max(0.0),
            (h - 2.0 * border_w).max(0.0),
        ],
        theme.f32(theme.black),
        (radius - border_w).max(0.0),
        ORDER_PANEL + 1,
        viewport_clip,
    );
    draw_top_rounded_rect_clipped(
        sugarloaf,
        [
            x + border_w,
            y + border_w,
            (w - 2.0 * border_w).max(0.0),
            (header_h - border_w).max(0.0),
        ],
        theme.f32(theme.surface),
        (radius - border_w).max(0.0),
        ORDER_PANEL + 2,
        viewport_clip,
    );
    draw_rect_clipped(
        sugarloaf,
        [
            x + border_w,
            y + header_h - border_w,
            (w - 2.0 * border_w).max(0.0),
            border_w,
        ],
        theme.f32(theme.border),
        ORDER_PANEL + 4,
        viewport_clip,
    );

    // Clip both the label and the code body to the timeline viewport so
    // nothing sneaks under the input chrome when a long block straddles
    // the bottom of the visible area.
    let header_label = lang_display_name(lang);
    if let Some(header_opts) = opts_with_clip(
        DrawOpts {
            font_size: 11.0 * s,
            color: theme.u8(theme.syn_type),
            bold: true,
            ..DrawOpts::default()
        },
        viewport_clip,
    ) {
        draw_text_clipped(
            sugarloaf,
            x + 14.0 * s,
            y + 6.0 * s,
            header_label,
            &header_opts,
            occlusion_rects,
        );
        let copy_hovered = !suppress_interactions && pane.link_hovered(copy_target);
        let copy_label = if copy_hovered { "Copy code" } else { "Copy" };
        let mut copy_opts = header_opts;
        copy_opts.color = theme.u8(theme.muted);
        copy_opts.bold = false;
        let copy_w = measure_text_cached(sugarloaf, copy_label, &copy_opts);
        let copy_x = (x + w - copy_w - 14.0 * s).max(x + 14.0 * s);
        draw_text_clipped(
            sugarloaf,
            copy_x,
            y + 6.0 * s,
            copy_label,
            &copy_opts,
            occlusion_rects,
        );
        if !suppress_interactions {
            register_copy_lines(copy_target, lines.clone());
            draw_hover_underline(
                sugarloaf,
                pane,
                copy_target,
                [copy_x, y + 20.0 * s, copy_w, 1.0 * s],
                theme,
                viewport_clip,
            );
            pane.register_link_hit_rect(
                copy_target.to_string(),
                [copy_x - 6.0 * s, y, copy_w + 12.0 * s, header_h],
            );
        }
    }

    // Line numbers — synthetic for fenced markdown blocks (no real source
    // offset). Width tracks how many digits the line count needs so wide
    // blocks don't waste space.
    let total_lines = lines.len().max(1);
    let digits = digit_count(total_lines).max(2);
    let gutter_pad_l = 12.0 * s;
    let gutter_pad_r = 10.0 * s;
    let num_text_w = (digits as f32) * 7.8 * s;
    let code_left_pad = gutter_pad_l + num_text_w + gutter_pad_r;
    let Some(opts) = opts_with_clip(
        DrawOpts {
            font_size: 12.5 * s,
            color: theme.u8(if body_muted { theme.muted } else { theme.fg }),
            bold: true,
            clip_rect: Some([
                x + code_left_pad,
                body_top,
                (w - code_left_pad - 12.0 * s).max(0.0),
                (h - header_h).max(0.0),
            ]),
            ..DrawOpts::default()
        },
        viewport_clip,
    ) else {
        return;
    };
    let Some(num_opts) = opts_with_clip(
        DrawOpts {
            font_size: 11.5 * s,
            color: theme.u8(theme.dim),
            clip_rect: Some([
                x + gutter_pad_l,
                body_top,
                num_text_w,
                (h - header_h).max(0.0),
            ]),
            ..DrawOpts::default()
        },
        viewport_clip,
    ) else {
        return;
    };
    let lang_id = syntax_lang(lang);
    let empty_line = String::new();
    for ix in start_ix..end_ix {
        let line = if lines.is_empty() {
            &empty_line
        } else if let Some(line) = lines.get(ix) {
            line
        } else {
            break;
        };
        let line_y = first_line_y + ix as f32 * line_h;
        let diff = diff_line_kind(line);
        render_code_line_background(
            sugarloaf,
            x,
            line_y,
            w,
            code_left_pad,
            diff,
            theme,
            s,
            viewport_clip,
        );
        let mut line_num_opts = num_opts;
        if let Some(color) = diff.map(|kind| super::code_block::diff_color(kind, theme)) {
            line_num_opts.color = theme.u8(color);
        }
        draw_text_clipped(
            sugarloaf,
            x + gutter_pad_l,
            line_y,
            &format!("{}", ix + 1),
            &line_num_opts,
            occlusion_rects,
        );
        render_code_line_text(
            sugarloaf,
            x + code_left_pad,
            line_y,
            line,
            lang_id,
            diff,
            &opts,
            theme,
            occlusion_rects,
        );
    }
}

fn lang_display_name(lang: &str) -> &'static str {
    match lang.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => "rust",
        "ts" | "typescript" => "typescript",
        "tsx" => "tsx",
        "js" | "javascript" => "javascript",
        "jsx" => "jsx",
        "py" | "python" => "python",
        "go" => "go",
        "lua" => "lua",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" | "jsonc" => "json",
        "md" | "markdown" => "markdown",
        "html" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "sql" => "sql",
        "sh" | "bash" | "zsh" => "shell",
        "c" => "c",
        "cpp" | "cxx" | "cc" | "c++" => "c++",
        "java" => "java",
        "kotlin" | "kt" => "kotlin",
        "swift" => "swift",
        "ruby" | "rb" => "ruby",
        "php" => "php",
        "diff" | "patch" => "diff",
        "" => "code",
        _ => "code",
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_markdown_inline_line<P: AgentMarkdownPane>(
    sugarloaf: &mut Sugarloaf,
    pane: &mut P,
    mut x: f32,
    y: f32,
    line: &str,
    opts: &DrawOpts,
    theme: &IdeTheme,
    suppress_interactions: bool,
    viewport_clip: [f32; 4],
    occlusion_rects: &[[f32; 4]],
) {
    if !suppress_interactions {
        let line_w = measure_text_cached(sugarloaf, line, opts).max(12.0);
        let line_index = pane
            .register_selectable_line(line, [x, y - 3.0, line_w, opts.font_size + 8.0]);
        if let Some((sel_left, sel_right)) = pane.selectable_line_highlight(line_index) {
            // Sub-line highlight follows the drag end-points so the user can
            // grab a single word instead of being forced into a full row.
            let pad = 2.0;
            let hl_x = (sel_left - pad).max(x - 4.0);
            let hl_w = (sel_right - sel_left + pad * 2.0).max(2.0);
            draw_rounded_rect_clipped(
                sugarloaf,
                [hl_x, y - 3.0, hl_w, opts.font_size + 8.0],
                theme.f32_alpha(theme.accent, 0.22),
                4.0,
                ORDER_PANEL + 2,
                viewport_clip,
            );
        }
    }
    let segments = parsed_markdown_inline_line(line);
    for segment in segments.iter() {
        match segment {
            MarkdownInlineSegment::Text(text) => {
                draw_text_clipped(sugarloaf, x, y, text, opts, occlusion_rects);
                x += measure_text_cached(sugarloaf, text, opts);
            }
            MarkdownInlineSegment::Bold(text) => {
                let mut bold = *opts;
                bold.bold = true;
                bold.color = theme.u8(theme.white);
                draw_text_clipped(sugarloaf, x, y, text, &bold, occlusion_rects);
                x += measure_text_cached(sugarloaf, text, &bold);
            }
            MarkdownInlineSegment::Strike(text) => {
                let mut strike_opts = *opts;
                strike_opts.color = theme.u8(theme.muted);
                draw_text_clipped(sugarloaf, x, y, text, &strike_opts, occlusion_rects);
                let w = measure_text_cached(sugarloaf, text, &strike_opts);
                draw_rect_clipped(
                    sugarloaf,
                    [
                        x,
                        y + opts.font_size * 0.55,
                        w,
                        1.25 * opts.font_size.max(1.0) / 13.5,
                    ],
                    rgba_from_u8(strike_opts.color),
                    ORDER_TEXT,
                    viewport_clip,
                );
                x += w;
            }
            MarkdownInlineSegment::Code { text, target } => {
                let mut code = *opts;
                code.bold = true;
                code.color = theme.u8(if target.is_some() {
                    theme.blue
                } else {
                    theme.syn_string
                });
                draw_text_clipped(sugarloaf, x, y, text, &code, occlusion_rects);
                x += measure_text_cached(sugarloaf, text, &code);
                if !suppress_interactions {
                    let Some(target) = target.as_ref() else {
                        continue;
                    };
                    let w = measure_text_cached(sugarloaf, text, &code);
                    draw_hover_underline(
                        sugarloaf,
                        pane,
                        &target,
                        [x - w, y + opts.font_size + 2.0, w, 1.0],
                        theme,
                        viewport_clip,
                    );
                    pane.register_link_hit_rect(
                        target.clone(),
                        [x - w, y - 2.0, w, opts.font_size + 8.0],
                    );
                }
            }
            MarkdownInlineSegment::MarkdownLink { label, target } => {
                let mut link_opts = *opts;
                link_opts.color = theme.u8(if target.is_some() {
                    theme.blue
                } else {
                    theme.cyan
                });
                draw_text_clipped(sugarloaf, x, y, label, &link_opts, occlusion_rects);
                let w = measure_text_cached(sugarloaf, label, &link_opts);
                if !suppress_interactions {
                    let Some(target) = target.as_ref() else {
                        x += w;
                        continue;
                    };
                    draw_hover_underline(
                        sugarloaf,
                        pane,
                        &target,
                        [x, y + opts.font_size + 2.0, w, 1.0],
                        theme,
                        viewport_clip,
                    );
                    pane.register_link_hit_rect(
                        target.clone(),
                        [x, y - 2.0, w, opts.font_size + 8.0],
                    );
                }
                x += w;
            }
            MarkdownInlineSegment::PlainToken {
                text,
                target,
                style,
            } => {
                let mut token_opts = *opts;
                if target.is_some() {
                    token_opts.color = theme.u8(theme.blue);
                } else if let Some(style) = style {
                    token_opts.color = theme.u8(plain_token_color(*style, theme));
                    token_opts.bold = style.bold;
                }
                draw_text_clipped(sugarloaf, x, y, text, &token_opts, occlusion_rects);
                let w = measure_text_cached(sugarloaf, text, &token_opts);
                if !suppress_interactions {
                    let Some(target) = target.as_ref() else {
                        x += w;
                        continue;
                    };
                    draw_hover_underline(
                        sugarloaf,
                        pane,
                        &target,
                        [x, y + opts.font_size + 2.0, w, 1.0],
                        theme,
                        viewport_clip,
                    );
                    pane.register_link_hit_rect(
                        target.clone(),
                        [x, y - 2.0, w, opts.font_size + 8.0],
                    );
                }
                x += w;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_markdown_table<P: AgentMarkdownPane>(
    sugarloaf: &mut Sugarloaf,
    pane: &mut P,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rows: &[Vec<String>],
    theme: &IdeTheme,
    s: f32,
    suppress_interactions: bool,
    viewport_clip: [f32; 4],
    occlusion_rects: &[[f32; 4]],
) {
    if h <= 0.0 || rows.is_empty() {
        return;
    }
    let cols = table_column_count(rows);
    let row_heights = laid_out_table_row_heights(rows, s);
    let table_h = row_heights.iter().sum::<f32>().min(h);
    let col_w = w / cols as f32;
    let border = theme.f32(theme.fg);
    draw_rect_clipped(
        sugarloaf,
        [x, y, w, 1.0 * s],
        border,
        ORDER_TEXT,
        viewport_clip,
    );
    draw_rect_clipped(
        sugarloaf,
        [x, y + table_h, w, 1.0 * s],
        border,
        ORDER_TEXT,
        viewport_clip,
    );
    for col in 0..=cols {
        let cx = x + col as f32 * col_w;
        draw_rect_clipped(
            sugarloaf,
            [cx, y, 1.0 * s, table_h],
            border,
            ORDER_TEXT,
            viewport_clip,
        );
    }
    let Some(cell_opts) = opts_with_clip(
        DrawOpts {
            font_size: 14.0 * s,
            color: theme.u8(theme.fg),
            ..DrawOpts::default()
        },
        viewport_clip,
    ) else {
        return;
    };
    let mut row_y = y;
    for (row_ix, row) in rows.iter().enumerate() {
        let row_h = row_heights
            .get(row_ix)
            .copied()
            .unwrap_or_else(|| table_row_height_for_lines(1, s));
        if row_y >= y + h {
            break;
        }
        draw_rect_clipped(
            sugarloaf,
            [x, row_y, w, 1.0 * s],
            border,
            ORDER_TEXT,
            viewport_clip,
        );
        let mut opts = cell_opts;
        if row_ix == 0 {
            opts.color = theme.u8(theme.muted);
        }
        for col in 0..cols {
            let cell = row.get(col).map(String::as_str).unwrap_or_default();
            let mut cell_opts = opts;
            if md::looks_like_file_ref(cell) {
                cell_opts.color = theme.u8(theme.blue);
            }
            for (line_ix, line) in table_cell_lines(cell).iter().enumerate() {
                let line_y =
                    row_y + TABLE_ROW_PAD_Y * s + line_ix as f32 * TABLE_CELL_LINE_H * s;
                if line_y + TABLE_CELL_LINE_H * s > y + h {
                    break;
                }
                draw_markdown_inline_line(
                    sugarloaf,
                    pane,
                    x + col as f32 * col_w + 14.0 * s,
                    line_y,
                    line,
                    &cell_opts,
                    theme,
                    suppress_interactions,
                    viewport_clip,
                    occlusion_rects,
                );
            }
        }
        row_y += row_h;
    }
}

fn measure_laid_out_table_height(rows: &[Vec<String>], s: f32) -> f32 {
    TABLE_BLOCK_PAD_Y * s + laid_out_table_row_heights(rows, s).iter().sum::<f32>()
}

fn laid_out_table_row_heights(rows: &[Vec<String>], s: f32) -> Vec<f32> {
    let cols = table_column_count(rows);
    rows.iter()
        .map(|row| {
            let lines = (0..cols)
                .map(|col| {
                    row.get(col)
                        .map(|cell| table_cell_lines(cell).len())
                        .unwrap_or(1)
                })
                .max()
                .unwrap_or(1);
            table_row_height_for_lines(lines, s)
        })
        .collect()
}

fn table_cell_lines(cell: &str) -> Vec<String> {
    let lines: Vec<String> = cell
        .lines()
        .map(str::trim)
        .map(ToOwned::to_owned)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

pub fn copied_code_from_link_target(target: &str) -> Option<String> {
    if target.starts_with(COPY_REF_LINK_PREFIX) {
        return COPY_SOURCE_CACHE
            .with(|cache| cache.borrow().get(target))
            .map(|lines| join_markdown_lines(lines.as_slice()));
    }
    target
        .strip_prefix(COPY_LINK_PREFIX)
        .and_then(unescape_copy_target)
}

fn copy_ref_target_for_lines(lines: &[String]) -> String {
    format!(
        "{COPY_REF_LINK_PREFIX}{:016x}:{}",
        stable_hash_lines(lines),
        lines.iter().map(String::len).sum::<usize>()
    )
}

fn register_copy_lines(target: &str, lines: Rc<Vec<String>>) {
    if target.starts_with(COPY_REF_LINK_PREFIX) {
        COPY_SOURCE_CACHE.with(|cache| cache.borrow_mut().insert(target, lines));
    }
}

pub fn mermaid_toggle_key_from_link_target(target: &str) -> Option<u64> {
    let hex = target.strip_prefix(MERMAID_TOGGLE_LINK_PREFIX)?;
    u64::from_str_radix(hex, 16).ok()
}

fn stable_hash(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn stable_hash_lines(lines: &[String]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for line in lines {
        for byte in line.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= b'\n' as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn escape_copy_target(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => {
                out.push('%');
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0f));
            }
        }
    }
    out
}

fn unescape_copy_target(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        if bytes[ix] == b'%' {
            let hi = *bytes.get(ix + 1)?;
            let lo = *bytes.get(ix + 2)?;
            out.push(hex_value(hi)? << 4 | hex_value(lo)?);
            ix += 3;
        } else {
            out.push(bytes[ix]);
            ix += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + value - 10) as char,
        _ => '0',
    }
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = trimmed.get(level..)?.trim_start();
    (!rest.is_empty()).then_some((level, rest))
}

fn markdown_bullet(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    for marker in ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some(rest.trim());
        }
    }
    let (number, rest) = trimmed.split_once(". ")?;
    (!number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()))
        .then_some(rest.trim())
}

fn markdown_quote(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix('>').map(str::trim)
}

fn expand_inline_bullets(line: &str) -> Vec<String> {
    let trimmed = line.trim_start();
    if markdown_bullet(trimmed).is_some() || !line.contains(" - ") {
        return vec![line.to_string()];
    }
    let Some(first_marker) = line.find(" - ") else {
        return vec![line.to_string()];
    };
    let before = line[..first_marker].trim_end();
    let rest = &line[first_marker + 3..];
    let mut out = Vec::new();
    if !before.is_empty() {
        out.push(before.to_string());
    }
    for item in rest.split(" - ") {
        let item = item.trim();
        if !item.is_empty() {
            out.push(format!("- {item}"));
        }
    }
    if out.is_empty() {
        vec![line.to_string()]
    } else {
        out
    }
}

#[cfg(test)]
mod tests;
