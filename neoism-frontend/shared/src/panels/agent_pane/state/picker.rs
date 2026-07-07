    use web_time::Instant;

    use crate::animation::CriticallyDampedSpring;

    const PICKER_VISIBLE_ROWS: usize = 8;
    const PICKER_ROW_HEIGHT: f32 = 34.0;
    const LIST_SCROLL_ANIMATION_LENGTH: f32 = 0.14;
    const CURSOR_ANIMATION_LENGTH: f32 = 0.10;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NeoismAgentPickerKind {
        Slash,
        Agent,
        Model,
        FileMention,
        SkillMention,
        Thinking,
        Session,
        Subagent,
        Skill,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct NeoismAgentPickerOption {
        pub title: String,
        pub description: String,
        pub footer: String,
        pub value: String,
        pub section: String,
        pub is_header: bool,
        /// True for the session the user is currently inside — renders
        /// a colored dot in the picker to distinguish "where I am" from
        /// the keyboard-selected row.
        pub is_current: bool,
    }

    impl NeoismAgentPickerOption {
        pub fn new(title: &str, description: &str, footer: &str, value: &str) -> Self {
            Self {
                title: title.to_string(),
                description: description.to_string(),
                footer: footer.to_string(),
                value: value.to_string(),
                section: String::new(),
                is_header: false,
                is_current: false,
            }
        }

        pub fn model(title: &str, provider: &str, footer: &str, value: &str) -> Self {
            let mut option = Self::new(title, "", footer, value);
            option.section = provider.to_string();
            option
        }

        pub fn header(title: &str) -> Self {
            Self {
                title: title.to_string(),
                description: String::new(),
                footer: String::new(),
                value: String::new(),
                section: title.to_string(),
                is_header: true,
                is_current: false,
            }
        }

        pub fn is_selectable(&self) -> bool {
            !self.is_header
        }
    }

    #[derive(Clone, Debug)]
    pub struct NeoismAgentPicker {
        pub kind: NeoismAgentPickerKind,
        pub title: String,
        pub query: String,
        pub selected: usize,
        all_options: Vec<NeoismAgentPickerOption>,
        filtered_options: Vec<NeoismAgentPickerOption>,
        pub scroll_offset: usize,
        pub last_rect: Option<[f32; 4]>,
        wheel_accumulator: f32,
        list_scroll_spring: CriticallyDampedSpring,
        cursor_spring: CriticallyDampedSpring,
        last_list_scroll_frame: Instant,
        last_cursor_frame: Instant,
    }

    impl NeoismAgentPicker {
        pub fn new(
            kind: NeoismAgentPickerKind,
            title: &str,
            options: Vec<NeoismAgentPickerOption>,
            selected: usize,
        ) -> Self {
            let selected = selectable_index_near(&options, selected).unwrap_or(0);
            Self {
                kind,
                title: title.to_string(),
                query: String::new(),
                selected,
                filtered_options: options.clone(),
                all_options: options,
                scroll_offset: 0,
                last_rect: None,
                wheel_accumulator: 0.0,
                list_scroll_spring: CriticallyDampedSpring::new(),
                cursor_spring: CriticallyDampedSpring::new(),
                last_list_scroll_frame: Instant::now(),
                last_cursor_frame: Instant::now(),
            }
        }

        pub fn options(&self) -> &[NeoismAgentPickerOption] {
            &self.filtered_options
        }

        pub fn selected_option(&self) -> Option<&NeoismAgentPickerOption> {
            self.filtered_options
                .get(self.selected)
                .filter(|option| option.is_selectable())
        }

        pub fn move_selection(&mut self, delta: isize) {
            let count = self.filtered_options.len();
            if count == 0 {
                self.selected = 0;
                return;
            }
            let Some(next) =
                selectable_step(&self.filtered_options, self.selected, delta)
            else {
                return;
            };
            self.set_selected(next);
        }

        fn set_selected(&mut self, next: usize) {
            if next == self.selected || next >= self.filtered_options.len() {
                return;
            }
            let was_idle = self.cursor_spring.position == 0.0;
            let rows = self.selected as i32 - next as i32;
            self.cursor_spring.position += rows as f32 * PICKER_ROW_HEIGHT;
            if was_idle {
                self.last_cursor_frame = Instant::now();
            }
            self.selected = next;
            self.clamp_scroll();
        }

        pub fn set_last_rect(&mut self, rect: [f32; 4]) {
            self.last_rect = Some(rect);
        }

        /// Translate a click into a row index and select+return true. The
        /// caller is expected to commit the picker; we just move the cursor.
        /// Header / row ratios mirror `renderer::inline_picker` (`TITLE_H = 30`,
        /// `ROW_H = PICKER_ROW_HEIGHT = 34`); we derive the per-row pixel
        /// height from the cached rect so the live scale factor doesn't need
        /// to be plumbed through.
        pub fn activate_row_at(&mut self, x: f32, y: f32) -> bool {
            let Some([rx, ry, rw, rh]) = self.last_rect else {
                return false;
            };
            if x < rx || x > rx + rw || y < ry || y > ry + rh {
                return false;
            }
            const HEADER_BASE: f32 = 30.0;
            let visible_rows =
                self.filtered_options.len().min(PICKER_VISIBLE_ROWS).max(1);
            let total_h = rh.max(1.0);
            let header_ratio =
                HEADER_BASE / (HEADER_BASE + PICKER_ROW_HEIGHT * visible_rows as f32);
            let header_h_px = total_h * header_ratio;
            let body_top = ry + header_h_px;
            if y < body_top {
                return false;
            }
            let row_h_px = (total_h - header_h_px) / visible_rows.max(1) as f32;
            if row_h_px <= 0.0 {
                return false;
            }
            let row_within = ((y - body_top) / row_h_px).floor() as usize;
            let target = self.scroll_offset + row_within;
            if target >= self.filtered_options.len() {
                return false;
            }
            if !self.filtered_options[target].is_selectable() {
                return false;
            }
            self.set_selected(target);
            true
        }

        pub fn contains_point(&self, x: f32, y: f32) -> bool {
            let Some([rx, ry, rw, rh]) = self.last_rect else {
                return false;
            };
            x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
        }

        pub fn scroll_pixels(&mut self, delta_pixels: f32) -> bool {
            let count = self.filtered_options.len();
            if count <= PICKER_VISIBLE_ROWS || delta_pixels == 0.0 {
                return false;
            }
            self.wheel_accumulator += delta_pixels;
            let mut rows = 0i32;
            while self.wheel_accumulator.abs() >= PICKER_ROW_HEIGHT {
                let sign = self.wheel_accumulator.signum();
                self.wheel_accumulator -= sign * PICKER_ROW_HEIGHT;
                rows += if sign > 0.0 { -1 } else { 1 };
            }
            if rows == 0 {
                return true;
            }
            let max_offset = count.saturating_sub(PICKER_VISIBLE_ROWS);
            let next = if rows < 0 {
                self.scroll_offset
                    .saturating_sub(rows.unsigned_abs() as usize)
            } else {
                self.scroll_offset
                    .saturating_add(rows as usize)
                    .min(max_offset)
            };
            self.set_scroll_offset(next);
            self.clamp_selected_to_viewport();
            true
        }

        pub fn tick_list_scroll(&mut self) -> f32 {
            if self.list_scroll_spring.position == 0.0 {
                self.last_list_scroll_frame = Instant::now();
                return 0.0;
            }
            let now = Instant::now();
            let dt = now
                .saturating_duration_since(self.last_list_scroll_frame)
                .as_secs_f32()
                .min(0.05);
            self.last_list_scroll_frame = now;
            self.list_scroll_spring
                .update(dt, LIST_SCROLL_ANIMATION_LENGTH);
            self.list_scroll_spring.position
        }

        pub fn tick_cursor(&mut self) -> f32 {
            if self.cursor_spring.position == 0.0 {
                self.last_cursor_frame = Instant::now();
                return 0.0;
            }
            let now = Instant::now();
            let dt = now
                .saturating_duration_since(self.last_cursor_frame)
                .as_secs_f32()
                .min(0.05);
            self.last_cursor_frame = now;
            self.cursor_spring.update(dt, CURSOR_ANIMATION_LENGTH);
            self.cursor_spring.position
        }

        pub fn is_animating(&self) -> bool {
            self.list_scroll_spring.position != 0.0 || self.cursor_spring.position != 0.0
        }

        fn set_scroll_offset(&mut self, next: usize) {
            let max_offset = self
                .filtered_options
                .len()
                .saturating_sub(PICKER_VISIBLE_ROWS);
            let next = next.min(max_offset);
            if next == self.scroll_offset {
                return;
            }
            let old = self.scroll_offset;
            self.scroll_offset = next;
            let was_idle = self.list_scroll_spring.position == 0.0;
            let rows = next as i32 - old as i32;
            self.list_scroll_spring.position += rows as f32 * PICKER_ROW_HEIGHT;
            if was_idle {
                self.last_list_scroll_frame = Instant::now();
            }
        }

        fn clamp_scroll(&mut self) {
            let count = self.filtered_options.len();
            if count == 0 {
                self.set_scroll_offset(0);
                return;
            }
            let visible = count.min(PICKER_VISIBLE_ROWS).max(1);
            if self.selected < self.scroll_offset {
                self.set_scroll_offset(self.selected);
            } else if self.selected >= self.scroll_offset + visible {
                self.set_scroll_offset(self.selected + 1 - visible);
            }
        }

        fn clamp_selected_to_viewport(&mut self) {
            let count = self.filtered_options.len();
            if count == 0 {
                self.selected = 0;
                return;
            }
            let visible = count.min(PICKER_VISIBLE_ROWS).max(1);
            let first = self.scroll_offset.min(count - 1);
            let last = (self.scroll_offset + visible - 1).min(count - 1);
            let old = self.selected;
            self.selected = selectable_index_between(
                &self.filtered_options,
                self.selected.clamp(first, last),
                first,
                last,
            )
            .or_else(|| selectable_index_near(&self.filtered_options, self.selected))
            .unwrap_or(0);
            if self.selected != old {
                // Kick the cursor spring so the highlight animates from
                // the old row to the new clamped row, rather than snapping
                // (which read as a "jump to top-left" when the highlight
                // was at the top/bottom of the viewport).
                let rows = old as i32 - self.selected as i32;
                let was_idle = self.cursor_spring.position == 0.0;
                self.cursor_spring.position += rows as f32 * PICKER_ROW_HEIGHT;
                if was_idle {
                    self.last_cursor_frame = Instant::now();
                }
            }
        }

        pub fn set_pre_filtered_options(
            &mut self,
            query: String,
            options: Vec<NeoismAgentPickerOption>,
        ) {
            self.query = query;
            self.filtered_options = options.clone();
            self.all_options = options;
            self.selected = self
                .selected
                .min(self.filtered_options.len().saturating_sub(1));
            self.selected =
                selectable_index_near(&self.filtered_options, self.selected).unwrap_or(0);
            self.scroll_offset = 0;
            self.wheel_accumulator = 0.0;
            self.list_scroll_spring.reset();
            self.cursor_spring.reset();
        }

        pub fn replace_options(&mut self, options: Vec<NeoismAgentPickerOption>) {
            let previous_value = self
                .filtered_options
                .get(self.selected)
                .filter(|option| option.is_selectable())
                .map(|option| option.value.clone());
            self.all_options = options;
            // Re-filter the NEW options against the current query directly.
            // `set_query()` short-circuits when the query is unchanged (a
            // keystroke-perf guard), so the old clear()+set_query() round-trip
            // silently failed to rebuild `filtered_options` whenever the query
            // was empty — leaving the picker showing its stale (e.g. "Loading
            // sessions…") rows after the real catalog arrived.
            self.rebuild_filtered_options();
            if let Some(value) = previous_value {
                if let Some(index) = self
                    .filtered_options
                    .iter()
                    .position(|option| option.value == value)
                {
                    self.selected = index;
                }
            }
            self.selected =
                selectable_index_near(&self.filtered_options, self.selected).unwrap_or(0);
        }

        /// Rebuild `filtered_options` from `all_options` for the current
        /// `query`. The single source of truth for the picker filter, called
        /// by both `set_query` (after its idempotency guard) and
        /// `replace_options` (which must rebuild even when the query is
        /// unchanged).
        fn rebuild_filtered_options(&mut self) {
            let needle = self.query.trim().to_lowercase();
            if needle.is_empty() {
                self.filtered_options = self.all_options.clone();
                self.selected =
                    selectable_index_near(&self.filtered_options, self.selected)
                        .unwrap_or(0);
                return;
            }
            let words = needle.split_whitespace().collect::<Vec<_>>();
            let mut output = Vec::new();
            let mut pending_header: Option<NeoismAgentPickerOption> = None;
            let mut pending_header_matches = false;
            let mut emitted_header = false;
            for option in &self.all_options {
                if option.is_header {
                    pending_header_matches = option_matches(option, &words);
                    pending_header = Some(option.clone());
                    emitted_header = false;
                    continue;
                }
                let matches = option_matches(option, &words) || pending_header_matches;
                if !matches {
                    continue;
                }
                if let Some(header) = pending_header.as_ref() {
                    if !emitted_header {
                        output.push(header.clone());
                        emitted_header = true;
                    }
                }
                output.push(option.clone());
            }
            self.filtered_options = output;
            self.selected =
                selectable_index_near(&self.filtered_options, self.selected).unwrap_or(0);
        }

        pub fn set_query(&mut self, query: String) {
            // Idempotent guard — same query firing every keystroke as the
            // user navigates the picker would otherwise reset scroll +
            // cursor spring on every frame, snapping the highlight back to
            // the top-left at boundaries.
            if self.query == query {
                return;
            }
            self.query = query;
            let previous_value = self
                .filtered_options
                .get(self.selected)
                .filter(|option| option.is_selectable())
                .map(|option| option.value.clone());
            self.rebuild_filtered_options();
            // Keep the cursor on the same option across filter changes
            // when it survives the filter, so the trail-cursor doesn't
            // animate back to row 0 every time the user types another
            // letter while their selection is mid-list.
            let new_selected = previous_value
                .and_then(|value| {
                    self.filtered_options
                        .iter()
                        .position(|option| option.value == value)
                })
                .and_then(|index| selectable_index_near(&self.filtered_options, index))
                .unwrap_or_else(|| {
                    selectable_index_near(&self.filtered_options, 0).unwrap_or(0)
                });
            let selection_changed = new_selected != self.selected;
            self.selected =
                new_selected.min(self.filtered_options.len().saturating_sub(1));
            self.scroll_offset = 0;
            self.wheel_accumulator = 0.0;
            if selection_changed {
                self.list_scroll_spring.reset();
                self.cursor_spring.reset();
            }
        }
    }

    fn selectable_index_near(
        options: &[NeoismAgentPickerOption],
        index: usize,
    ) -> Option<usize> {
        if options.is_empty() {
            return None;
        }
        let index = index.min(options.len().saturating_sub(1));
        if options
            .get(index)
            .is_some_and(NeoismAgentPickerOption::is_selectable)
        {
            return Some(index);
        }
        let last = options.len().saturating_sub(1);
        for offset in 0..=last {
            let forward = index.saturating_add(offset);
            if forward <= last && options[forward].is_selectable() {
                return Some(forward);
            }
            if let Some(backward) = index.checked_sub(offset) {
                if options[backward].is_selectable() {
                    return Some(backward);
                }
            }
        }
        None
    }

    fn selectable_index_between(
        options: &[NeoismAgentPickerOption],
        index: usize,
        first: usize,
        last: usize,
    ) -> Option<usize> {
        if options.is_empty() || first > last {
            return None;
        }
        let index = index.min(options.len().saturating_sub(1));
        if options
            .get(index)
            .is_some_and(NeoismAgentPickerOption::is_selectable)
        {
            return Some(index);
        }
        for offset in 0..=last.saturating_sub(first) {
            let forward = index.saturating_add(offset);
            if forward <= last
                && options
                    .get(forward)
                    .is_some_and(NeoismAgentPickerOption::is_selectable)
            {
                return Some(forward);
            }
            if let Some(backward) = index.checked_sub(offset) {
                if backward >= first
                    && options
                        .get(backward)
                        .is_some_and(NeoismAgentPickerOption::is_selectable)
                {
                    return Some(backward);
                }
            }
        }
        None
    }

    fn selectable_step(
        options: &[NeoismAgentPickerOption],
        selected: usize,
        delta: isize,
    ) -> Option<usize> {
        if options.is_empty() || delta == 0 {
            return selectable_index_near(options, selected);
        }
        let mut index = selected.min(options.len().saturating_sub(1));
        let mut remaining = delta.unsigned_abs().max(1);
        while remaining > 0 {
            let mut next = None;
            if delta > 0 {
                for candidate in index.saturating_add(1)..options.len() {
                    if options[candidate].is_selectable() {
                        next = Some(candidate);
                        break;
                    }
                }
            } else {
                for candidate in (0..index).rev() {
                    if options[candidate].is_selectable() {
                        next = Some(candidate);
                        break;
                    }
                }
            }
            index = next?;
            remaining -= 1;
        }
        Some(index)
    }

    fn option_matches(option: &NeoismAgentPickerOption, words: &[&str]) -> bool {
        let mut haystack = String::with_capacity(
            option.title.len()
                + option.description.len()
                + option.footer.len()
                + option.value.len()
                + option.section.len()
                + 4,
        );
        haystack.push_str(&option.title);
        haystack.push(' ');
        haystack.push_str(&option.description);
        haystack.push(' ');
        haystack.push_str(&option.footer);
        haystack.push(' ');
        haystack.push_str(&option.value);
        haystack.push(' ');
        haystack.push_str(&option.section);
        haystack.make_ascii_lowercase();
        words.iter().all(|word| haystack.contains(word))
    }
