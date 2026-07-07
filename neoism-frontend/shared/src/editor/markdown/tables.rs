use super::helpers::*;
use super::source_map::InlineSourceMap;
use super::types::*;

impl MarkdownPane {
    pub fn move_table_cell(&mut self, backwards: bool) -> bool {
        let Some(cursor) = self.table_cursor() else {
            return false;
        };
        let mut row_pos = cursor.row_pos;
        let mut cell_ix = cursor.cell_ix;
        if backwards {
            if cell_ix > 0 {
                cell_ix -= 1;
            } else if row_pos > 0 {
                row_pos -= 1;
                let line = cursor.editable_lines[row_pos];
                cell_ix = parse_table_cell_bounds(&self.lines[line])
                    .map(|cells| cells.len().saturating_sub(1))
                    .unwrap_or(0);
            } else {
                return false;
            }
        } else {
            let current_cell_count =
                parse_table_cell_bounds(&self.lines[self.cursor_line])
                    .map(|cells| cells.len())
                    .unwrap_or(0);
            if cell_ix + 1 < current_cell_count {
                cell_ix += 1;
            } else if row_pos + 1 < cursor.editable_lines.len() {
                row_pos += 1;
                cell_ix = 0;
            } else {
                return false;
            }
        }

        self.set_cursor_to_table_cell(cursor.editable_lines[row_pos], cell_ix, 0);
        true
    }

    pub fn insert_table_column(&mut self, start_line: usize, col_ix: usize) -> bool {
        let Some(range) = self.table_range_from_start(start_line) else {
            return false;
        };
        let col_count = self.table_col_count_for_range(&range).max(2);
        let insert_ix = col_ix.min(col_count);

        self.save_undo();
        let mut byte_delta = 0isize;
        for line_ix in range.clone() {
            let value = if line_ix == range.start + 1 {
                "---".to_string()
            } else if line_ix == range.start {
                format!("Column {}", insert_ix + 1)
            } else {
                " ".to_string()
            };
            if let Some(next) = table_line_with_inserted_cell(
                self.lines.get(line_ix).map(String::as_str).unwrap_or(""),
                insert_ix,
                &value,
            ) {
                byte_delta += next.len() as isize - self.lines[line_ix].len() as isize;
                self.lines[line_ix] = next;
            }
        }
        self.adjust_source_len(byte_delta);
        self.cursor_line = range.start;
        self.cursor_col = parse_table_cell_bounds(&self.lines[self.cursor_line])
            .and_then(|cells| cells.get(insert_ix).copied())
            .map(table_cell_entry_col)
            .unwrap_or(0);
        self.mode = MarkdownMode::Insert;
        self.visual_anchor = None;
        self.mouse_select_anchor = None;
        self.vim.clear_pending();
        self.follow_cursor = true;
        self.rebuild_blocks();
        self.commit_undo();
        true
    }

    pub fn move_table_row_fast(&mut self, down: bool) -> bool {
        self.table_cursor().is_some()
            && (self.move_table_row(down) || self.move_out_of_table(down))
    }

    pub fn insert_table_row(&mut self, above: bool) -> bool {
        let Some(cursor) = self.table_cursor() else {
            return false;
        };
        let col_count = self.table_col_count_for_range(&cursor.range).max(2);
        let insert_at = if above {
            if self.cursor_line == cursor.range.start {
                cursor.range.start + 2
            } else {
                self.cursor_line
            }
        } else if self.cursor_line == cursor.range.start {
            cursor.range.start + 2
        } else {
            self.cursor_line + 1
        }
        .min(self.lines.len());

        self.save_undo();
        self.shift_enter_continuations_for_insert(insert_at);
        let row = empty_table_row(col_count);
        let byte_delta = row.len() as i64 + 1;
        self.lines.insert(insert_at, row);
        self.adjust_source_len(byte_delta as isize);
        self.record_line_insert(insert_at, byte_delta);
        self.cursor_line = insert_at;
        self.cursor_col = parse_table_cell_bounds(&self.lines[self.cursor_line])
            .and_then(|cells| cells.first().copied())
            .map(table_cell_entry_col)
            .unwrap_or(0);
        self.mode = MarkdownMode::Insert;
        self.visual_anchor = None;
        self.mouse_select_anchor = None;
        self.follow_cursor = true;
        self.rebuild_blocks();
        self.commit_undo();
        true
    }

    pub fn insert_table_row_after(&mut self, after_line: usize) -> bool {
        let Some(range) = self.table_range_containing(after_line) else {
            return false;
        };
        let col_count = self.table_col_count_for_range(&range).max(2);
        let bounded_after = after_line.clamp(range.start, range.end.saturating_sub(1));
        let insert_at = if bounded_after <= range.start + 1 {
            range.start + 2
        } else {
            bounded_after + 1
        }
        .min(self.lines.len());

        self.save_undo();
        self.shift_enter_continuations_for_insert(insert_at);
        let row = empty_table_row(col_count);
        let byte_delta = row.len() as i64 + 1;
        self.lines.insert(insert_at, row);
        self.adjust_source_len(byte_delta as isize);
        self.record_line_insert(insert_at, byte_delta);
        self.cursor_line = insert_at;
        self.cursor_col = parse_table_cell_bounds(&self.lines[self.cursor_line])
            .and_then(|cells| cells.first().copied())
            .map(table_cell_entry_col)
            .unwrap_or(0);
        self.mode = MarkdownMode::Insert;
        self.visual_anchor = None;
        self.mouse_select_anchor = None;
        self.vim.clear_pending();
        self.follow_cursor = true;
        self.rebuild_blocks();
        self.commit_undo();
        true
    }

    pub(crate) fn backspace_table_boundary(&mut self) -> bool {
        let Some(cursor) = self.table_cursor() else {
            return false;
        };
        if self.remove_empty_table_row(cursor.range.clone(), self.cursor_line) {
            return true;
        }
        let Some(bounds) = parse_table_cell_bounds(&self.lines[self.cursor_line])
            .and_then(|cells| cells.get(cursor.cell_ix).copied())
        else {
            return false;
        };
        if self.cursor_col > table_cell_entry_col(bounds)
            && self.cursor_col > bounds.content_start
        {
            return false;
        }
        if !self.move_table_cell(true) {
            self.follow_cursor = true;
        }
        true
    }

    pub(crate) fn delete_table_boundary(&mut self) -> bool {
        let Some(cursor) = self.table_cursor() else {
            return false;
        };
        if self.remove_empty_table_row(cursor.range.clone(), self.cursor_line) {
            return true;
        }
        let Some(bounds) = parse_table_cell_bounds(&self.lines[self.cursor_line])
            .and_then(|cells| cells.get(cursor.cell_ix).copied())
        else {
            return false;
        };
        if self.cursor_col < bounds.content_end {
            return false;
        }
        if !self.move_table_cell(false) {
            self.follow_cursor = true;
        }
        true
    }

    pub(crate) fn move_table_horizontal(&mut self, right: bool) -> Option<()> {
        self.clamp_cursor();
        let cursor = self.table_cursor()?;
        let cells = parse_table_cell_bounds(&self.lines[self.cursor_line])?;
        let bounds = cells.get(cursor.cell_ix).copied()?;
        if right {
            if self.cursor_col < bounds.content_end {
                self.cursor_col = next_char_boundary(
                    &self.lines[self.cursor_line],
                    self.cursor_col.max(bounds.content_start),
                )
                .min(bounds.content_end);
                self.follow_cursor = true;
            } else if let Some(next) = cells.get(cursor.cell_ix + 1).copied() {
                self.cursor_col = table_cell_entry_col(next);
                self.follow_cursor = true;
            } else {
                self.follow_cursor = true;
            }
        } else {
            let entry_col = table_cell_entry_col(bounds);
            if self.cursor_col > entry_col {
                let prev =
                    prev_char_boundary(&self.lines[self.cursor_line], self.cursor_col);
                self.cursor_col = prev.max(entry_col);
                self.follow_cursor = true;
            } else if cursor.cell_ix > 0 {
                let previous = cells[cursor.cell_ix - 1];
                self.cursor_col = previous.content_end;
                self.follow_cursor = true;
            } else {
                self.follow_cursor = true;
            }
        }
        Some(())
    }

    pub(crate) fn move_table_row(&mut self, down: bool) -> bool {
        let Some(cursor) = self.table_cursor() else {
            return false;
        };
        let next_pos = if down {
            cursor.row_pos + 1
        } else {
            let Some(previous) = cursor.row_pos.checked_sub(1) else {
                return false;
            };
            previous
        };
        let Some(&line) = cursor.editable_lines.get(next_pos) else {
            return false;
        };
        self.set_cursor_to_table_cell(line, cursor.cell_ix, cursor.cell_offset_chars);
        true
    }

    pub(crate) fn remove_empty_table_row(
        &mut self,
        range: std::ops::Range<usize>,
        line_ix: usize,
    ) -> bool {
        if line_ix <= range.start + 1 || !range.contains(&line_ix) {
            return false;
        }
        if !self
            .lines
            .get(line_ix)
            .is_some_and(|line| table_row_cells_empty(line))
        {
            return false;
        }

        self.save_undo();
        let removed = self.lines.remove(line_ix);
        self.shift_enter_continuations_for_remove(line_ix);
        let byte_delta = -((removed.len() + 1) as i64);
        self.adjust_source_len(byte_delta as isize);
        self.record_line_delete(line_ix, byte_delta);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        let target = if line_ix < range.end.saturating_sub(1) {
            line_ix
        } else {
            line_ix.saturating_sub(1)
        }
        .min(self.lines.len() - 1);
        self.cursor_line = if self.is_editable_line(target) {
            target
        } else {
            self.nearest_editable_line(target)
        };
        self.cursor_col = parse_table_cell_bounds(&self.lines[self.cursor_line])
            .and_then(|cells| cells.first().copied())
            .map(table_cell_entry_col)
            .unwrap_or_else(|| self.visible_start_col(self.cursor_line));
        self.visual_anchor = None;
        self.mouse_select_anchor = None;
        self.vim.clear_pending();
        self.follow_cursor = true;
        self.rebuild_blocks();
        self.commit_undo();
        true
    }

    pub(crate) fn move_table_visual_vertical(&mut self, down: bool) -> Option<bool> {
        self.clamp_cursor();
        let cursor = self.table_cursor()?;
        let cell_rect = self
            .table_cell_rects
            .iter()
            .find(|cell| cell.line == self.cursor_line && cell.cell_ix == cursor.cell_ix)
            .cloned()?;
        let bounds = parse_table_cell_bounds(&self.lines[self.cursor_line])
            .and_then(|cells| cells.get(cursor.cell_ix).copied())?;
        let (visual_line, visual_col) = table_cell_cursor_visual_position(
            &self.lines[self.cursor_line],
            bounds,
            self.cursor_col,
            &cell_rect.hit_rows,
        );
        let line_count = cell_rect.hit_rows.len().max(1);

        if down {
            if visual_line + 1 < line_count {
                self.set_table_cell_visual_position(
                    self.cursor_line,
                    bounds,
                    visual_line + 1,
                    visual_col,
                    &cell_rect.hit_rows,
                );
                return Some(true);
            }
        } else if visual_line > 0 {
            self.set_table_cell_visual_position(
                self.cursor_line,
                bounds,
                visual_line - 1,
                visual_col,
                &cell_rect.hit_rows,
            );
            return Some(true);
        }

        Some(false)
    }

    pub(crate) fn move_out_of_table(&mut self, down: bool) -> bool {
        let Some(range) = self.table_range_containing(self.cursor_line) else {
            return false;
        };
        let target = if down {
            (range.end..self.lines.len()).find(|line| self.is_editable_line(*line))
        } else if range.start == 0 {
            None
        } else {
            (0..range.start)
                .rev()
                .find(|line| self.is_editable_line(*line))
        };
        let Some(line) = target else {
            return false;
        };
        self.cursor_line = line;
        self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
        self.clamp_cursor();
        self.follow_cursor = true;
        true
    }

    pub(super) fn set_table_cell_visual_position(
        &mut self,
        line_ix: usize,
        bounds: MarkdownTableCellBounds,
        visual_line: usize,
        visual_col: usize,
        hit_rows: &[MarkdownWrapHitRow],
    ) {
        let Some(line) = self.lines.get(line_ix) else {
            return;
        };
        let visible_len = table_cell_visible_len(line, bounds);
        let offset = hit_rows
            .get(visual_line)
            .map(|row| row.start + visual_col.min(row.stops.len().saturating_sub(1)))
            .unwrap_or(visible_len)
            .min(visible_len);
        self.cursor_line = line_ix;
        let cell_source = &line[bounds.content_start..bounds.content_end];
        self.cursor_col = bounds.content_start
            + InlineSourceMap::new(cell_source)
                .source_for_visible(offset)
                .min(cell_source.len());
        self.clamp_cursor();
        self.follow_cursor = true;
    }

    pub(super) fn table_cursor(&self) -> Option<MarkdownTableCursor> {
        let range = self.table_range_containing(self.cursor_line)?;
        if self.cursor_line == range.start + 1 {
            return None;
        }
        let editable_lines = range
            .clone()
            .filter(|line| *line != range.start + 1)
            .collect::<Vec<_>>();
        let row_pos = editable_lines
            .iter()
            .position(|line| *line == self.cursor_line)?;
        let line = self.lines.get(self.cursor_line)?;
        let bounds = parse_table_cell_bounds(line)?;
        if bounds.is_empty() {
            return None;
        }
        let col = floor_char_boundary(line, self.cursor_col.min(line.len()));
        let cell_ix = bounds
            .iter()
            .position(|cell| col >= cell.raw_start && col <= cell.raw_end)
            .unwrap_or_else(|| nearest_table_cell_ix(&bounds, col));
        let cell = bounds[cell_ix];
        let cell_col = col.clamp(cell.content_start, cell.content_end);
        let cell_source = &line[cell.content_start..cell.content_end];
        let cell_offset_chars = InlineSourceMap::new(cell_source)
            .visible_for_source(cell_col - cell.content_start);
        Some(MarkdownTableCursor {
            range,
            editable_lines,
            row_pos,
            cell_ix,
            cell_offset_chars,
        })
    }

    pub(crate) fn set_cursor_to_table_cell(
        &mut self,
        line_ix: usize,
        cell_ix: usize,
        offset_chars: usize,
    ) {
        let Some(line) = self.lines.get(line_ix) else {
            return;
        };
        let Some(bounds) = parse_table_cell_bounds(line) else {
            return;
        };
        if bounds.is_empty() {
            return;
        }
        let ix = cell_ix.min(bounds.len() - 1);
        let cell = bounds[ix];
        self.cursor_line = line_ix;
        let cell_source = &line[cell.content_start..cell.content_end];
        self.cursor_col = cell.content_start
            + InlineSourceMap::new(cell_source)
                .source_for_visible(offset_chars)
                .min(cell_source.len());
        self.clamp_cursor();
        self.follow_cursor = true;
    }

    pub(crate) fn table_col_count_for_range(
        &self,
        range: &std::ops::Range<usize>,
    ) -> usize {
        range
            .clone()
            .filter(|line| *line != range.start + 1)
            .filter_map(|line| {
                self.lines
                    .get(line)
                    .and_then(|line| parse_table_cell_bounds(line))
                    .map(|cells| cells.len())
            })
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn table_range_from_start(
        &self,
        start_line: usize,
    ) -> Option<std::ops::Range<usize>> {
        parse_table_cells(self.lines.get(start_line)?)
            .filter(|cells| cells.len() >= 2)?;
        let separator = parse_table_cells(self.lines.get(start_line + 1)?)?;
        if !is_table_separator_cells(&separator) {
            return None;
        }
        let mut end = start_line + 2;
        while let Some(cells) =
            self.lines.get(end).and_then(|line| parse_table_cells(line))
        {
            if is_table_separator_cells(&cells) {
                break;
            }
            end += 1;
        }
        Some(start_line..end)
    }
}

fn table_cell_cursor_visual_position(
    line: &str,
    bounds: MarkdownTableCellBounds,
    cursor_col: usize,
    hit_rows: &[MarkdownWrapHitRow],
) -> (usize, usize) {
    let cell_col = floor_char_boundary(line, cursor_col.min(line.len()))
        .clamp(bounds.content_start, bounds.content_end);
    let cell_source = &line[bounds.content_start..bounds.content_end];
    let offset = InlineSourceMap::new(cell_source)
        .visible_for_source(cell_col - bounds.content_start);
    if offset == 0 {
        return (0, 0);
    }
    for (row_ix, row) in hit_rows.iter().enumerate() {
        let row_len = row.stops.len().saturating_sub(1);
        let row_end = row.start + row_len;
        if offset <= row_end || row_ix + 1 == hit_rows.len() {
            return (row_ix, offset.saturating_sub(row.start));
        }
    }
    (0, 0)
}
