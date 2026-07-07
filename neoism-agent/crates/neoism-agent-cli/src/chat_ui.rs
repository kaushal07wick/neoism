use std::collections::VecDeque;
use std::io::Write;

use crate::chat_input::{CompletionMenu, CompletionMode};
use crate::chat_markdown::ansi_visible_width;
pub(crate) use crate::chat_markdown::{highlight_code_line, truncate_for_terminal};
pub(crate) use crate::chat_terminal::{
    read_key, stdin_is_tty, terminal_size, try_read_key, Key, RawTerminal,
};
use crate::chat_tool_render::TruncatedOutput;
use crate::{
    BOLD, CLEAR_LINE, CYAN, DIM, GREEN, RED, RESET, SPINNER_FRAMES, WHITE, YELLOW,
};

const PROMPT_MARKER: &str = "›";
const BORDER_TL: &str = "╭";
const BORDER_TR: &str = "╮";
const BORDER_BL: &str = "╰";
const BORDER_BR: &str = "╯";
const BORDER_H: &str = "─";
const BORDER_V: &str = "│";
const FOOTER_ROWS: u16 = 1;
const WORKING_ROWS: u16 = 1;
const MAX_INPUT_ROWS: usize = 5;
const COMPOSER_BG: &str = "\x1b[48;2;24;24;27m";
const COMPOSER_TEXT: &str = "\x1b[38;2;238;238;238m";

#[derive(Clone, Debug)]
struct InputLayout {
    rows: Vec<String>,
}

enum FooterMode<'a> {
    Hints,
    Working { activity: &'a str, frame: &'a str },
}

#[derive(Clone, Debug)]
pub(crate) struct PickerItem {
    pub(crate) primary: String,
    pub(crate) secondary: String,
}

pub(crate) struct BottomPrompt {
    overlay_rows: u16,
    spinner: usize,
    pending_expansions: VecDeque<TruncatedOutput>,
}

impl BottomPrompt {
    pub(crate) fn new() -> Self {
        Self {
            overlay_rows: 0,
            spinner: 0,
            pending_expansions: VecDeque::new(),
        }
    }

    pub(crate) fn add_pending_expansions(&mut self, expansions: Vec<TruncatedOutput>) {
        if let Some(expansion) = expansions.into_iter().last() {
            self.pending_expansions.clear();
            self.pending_expansions.push_back(expansion);
        }
    }

    pub(crate) fn render_picker_search(
        &mut self,
        title: &str,
        query: &str,
        items: &[PickerItem],
        selected: usize,
    ) -> anyhow::Result<()> {
        let banner = if query.is_empty() {
            title.to_string()
        } else {
            format!("{title} · {YELLOW}{query}{RESET}{BOLD}{CYAN}")
        };
        self.render_picker(&banner, items, selected)
    }

    pub(crate) fn render_picker(
        &mut self,
        title: &str,
        items: &[PickerItem],
        selected: usize,
    ) -> anyhow::Result<()> {
        let (width, height) = terminal_size();
        print!("\x1b[?25l");
        self.clear_overlay()?;
        let max_visible: usize = 12;
        let visible = items.len().min(max_visible).max(1);
        let total_rows = (visible + 2) as u16;
        let footer_rows: u16 = 1;
        let total_reserved = total_rows + footer_rows;
        self.set_scroll_region(total_reserved)?;

        // Compute scroll window so selected stays in view
        let scroll = if items.len() <= visible {
            0
        } else if selected < visible / 2 {
            0
        } else if selected + visible / 2 >= items.len() {
            items.len() - visible
        } else {
            selected - visible / 2
        };

        let footer_row = height.max(1);
        let bottom_row = footer_row.saturating_sub(1).max(1);
        let top_row = bottom_row.saturating_sub(total_rows - 1).max(1);
        let inner_width = (width as usize).saturating_sub(2);

        // Top border with title
        let title_label = format!(" {title} ");
        let title_chars = ansi_visible_width(&title_label);
        let top_fill = inner_width.saturating_sub(title_chars + 1);
        print!(
            "\x1b[{top_row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_TL}{BORDER_H}{RESET}{BOLD}{CYAN}{title_label}{RESET}{DIM}{}{BORDER_TR}{RESET}",
            BORDER_H.repeat(top_fill)
        );

        // Body rows
        for r in 0..(visible as u16) {
            let row = top_row + 1 + r;
            let opt_idx = scroll + r as usize;
            if opt_idx >= items.len() {
                let empty = " ".repeat(inner_width);
                print!(
                    "\x1b[{row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_V}{RESET}{empty}{DIM}{BORDER_V}{RESET}"
                );
                continue;
            }
            let item = &items[opt_idx];
            let is_selected = opt_idx == selected;
            let primary_width: usize = inner_width.saturating_sub(28).max(8);
            let primary = truncate_for_terminal(&item.primary, primary_width);
            let secondary_width: usize =
                inner_width.saturating_sub(4 + primary_width + 1).max(4);
            let secondary = truncate_for_terminal(&item.secondary, secondary_width);

            let body = if is_selected {
                format!(
                    " {BOLD}{CYAN}▶{RESET} {BOLD}{CYAN}{primary:<primary_width$}{RESET} {DIM}{secondary:>secondary_width$}{RESET} "
                )
            } else {
                format!("   {primary:<primary_width$} {DIM}{secondary:>secondary_width$}{RESET} ")
            };
            print!(
                "\x1b[{row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_V}{RESET}{body}{DIM}{BORDER_V}{RESET}"
            );
        }

        // Bottom border with hint + count
        let hint = " ↑↓ navigate · enter open · esc cancel ";
        let count = format!(" {}/{} ", selected + 1, items.len());
        let hint_chars = hint.chars().count();
        let count_chars = count.chars().count();
        let fill = inner_width
            .saturating_sub(hint_chars + count_chars + 2)
            .max(1);
        print!(
            "\x1b[{bottom_row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_BL}{BORDER_H}{RESET}{DIM}{hint}{RESET}{DIM}{}{RESET}{DIM}{count}{BORDER_H}{BORDER_BR}{RESET}",
            BORDER_H.repeat(fill)
        );

        // Footer status line (just clean spacer)
        print!("\x1b[{footer_row};1H{RESET}{CLEAR_LINE}");

        std::io::stdout().flush()?;
        self.overlay_rows = total_reserved;
        Ok(())
    }

    pub(crate) fn expand_pending(&mut self) -> anyhow::Result<bool> {
        let Some(expansion) = self.pending_expansions.pop_back() else {
            return Ok(false);
        };
        self.before_output()?;
        println!();
        println!(
            " {GREEN}{BOLD}●{RESET} {BOLD}{}{RESET} {DIM}(expanded){RESET}",
            expansion.header
        );
        for (index, line) in expansion.lines.iter().enumerate() {
            if index == 0 {
                println!("  {DIM}└{RESET} {line}");
            } else {
                println!("    {line}");
            }
        }
        Ok(true)
    }

    pub(crate) fn render_prompt(
        &mut self,
        input: &str,
        menu: Option<&CompletionMenu>,
        right: &str,
    ) -> anyhow::Result<()> {
        let (width, height) = terminal_size();
        self.clear_overlay()?;
        let composer_rows = self.composer_reserved_rows(width, input);
        let menu_rows = self.render_menu(menu, width, height, composer_rows)?;
        let total_reserved = menu_rows + composer_rows;
        self.set_scroll_region(total_reserved)?;
        let footer_row = height.max(1);
        self.draw_input_panel(width, height, input)?;
        self.render_footer(footer_row, width, right, FooterMode::Hints)?;
        self.position_input_cursor(width, height, input);
        print!("\x1b[?25h");
        std::io::stdout().flush()?;
        self.overlay_rows = total_reserved;
        Ok(())
    }

    fn draw_input_panel(
        &self,
        width: u16,
        height: u16,
        input: &str,
    ) -> anyhow::Result<()> {
        let layout = input_layout(width, input);
        let input_rows = layout.rows.len() as u16;
        let first_row = height
            .saturating_sub(FOOTER_ROWS + input_rows)
            .saturating_add(1)
            .max(1);
        let text_width = text_capacity(width);
        for (index, line) in layout.rows.iter().enumerate() {
            let row = first_row + index as u16;
            let line = truncate_for_terminal(line, text_width);
            let marker = if index == 0 {
                format!("{COMPOSER_BG}{BOLD}{CYAN}{PROMPT_MARKER}{RESET}{COMPOSER_BG} ")
            } else {
                format!("{COMPOSER_BG}  ")
            };
            let body = format!("{marker}{COMPOSER_TEXT}{line}{RESET}{COMPOSER_BG}");
            self.print_composer_row(row, width, &body)?;
        }
        Ok(())
    }

    fn position_input_cursor(&self, width: u16, height: u16, input: &str) {
        let layout = input_layout(width, input);
        let input_rows = layout.rows.len() as u16;
        let first_row = height
            .saturating_sub(FOOTER_ROWS + input_rows)
            .saturating_add(1)
            .max(1);
        let cursor_row = first_row + input_rows.saturating_sub(1);
        let input_chars = layout
            .rows
            .last()
            .map(|line| line.chars().count())
            .unwrap_or_default();
        let cursor_col = (3 + input_chars)
            .min((width as usize).saturating_sub(1).max(1))
            .max(1);
        print!("\x1b[{cursor_row};{cursor_col}H");
    }

    fn composer_reserved_rows(&self, width: u16, input: &str) -> u16 {
        input_layout(width, input).rows.len() as u16 + FOOTER_ROWS
    }

    fn print_composer_row(&self, row: u16, width: u16, body: &str) -> anyhow::Result<()> {
        let body_width = ansi_visible_width(body);
        let padding = (width as usize).saturating_sub(body_width);
        print!(
            "\x1b[{row};1H{RESET}{CLEAR_LINE}{COMPOSER_BG}{body}{}{RESET}",
            " ".repeat(padding)
        );
        Ok(())
    }

    fn render_footer(
        &self,
        row: u16,
        width: u16,
        right: &str,
        mode: FooterMode,
    ) -> anyhow::Result<()> {
        match mode {
            FooterMode::Hints => self.render_footer_hints(row, width, right),
            FooterMode::Working { activity, frame } => {
                self.render_footer_working(row, width, activity, frame)
            }
        }
    }

    fn render_footer_hints(
        &self,
        row: u16,
        width: u16,
        right: &str,
    ) -> anyhow::Result<()> {
        let full_hints = if self.pending_expansions.is_empty() {
            "? help · / cmds · @ files · tab agent".to_string()
        } else {
            "? help · / cmds · @ files · tab agent · ctrl+o expand".to_string()
        };
        let compact_hints = if self.pending_expansions.is_empty() {
            "? · / · @ · tab".to_string()
        } else {
            "? · / · @ · tab · ^O".to_string()
        };
        let full_hints = full_hints.as_str();
        let compact_hints = compact_hints.as_str();
        let mut hints = full_hints;
        let mut right = truncate_for_terminal(right, width.saturating_sub(2) as usize);
        if full_hints.chars().count() + right.chars().count() + 4 > width as usize {
            hints = compact_hints;
        }
        let max_right = width
            .saturating_sub(hints.chars().count() as u16)
            .saturating_sub(4) as usize;
        right = truncate_for_terminal(&right, max_right);
        let hints_width = hints.chars().count() as u16;
        let right_width = right.chars().count() as u16;
        let gap = width
            .saturating_sub(hints_width)
            .saturating_sub(right_width)
            .saturating_sub(2)
            .max(1);
        print!(
            "\x1b[{row};1H{RESET}{CLEAR_LINE} {DIM}{hints}{RESET}{}{DIM}{right}{RESET} ",
            " ".repeat(gap as usize),
        );
        print!("{RESET}");
        Ok(())
    }

    fn render_footer_working(
        &self,
        row: u16,
        width: u16,
        _activity: &str,
        _frame: &str,
    ) -> anyhow::Result<()> {
        let hint = if self.pending_expansions.is_empty() {
            "esc cancel · / cmds · @ files".to_string()
        } else {
            "esc cancel · / cmds · @ files · ctrl+o expand".to_string()
        };
        let hint = truncate_for_terminal(&hint, width.saturating_sub(2) as usize);
        print!(
            "\x1b[{row};1H{RESET}{CLEAR_LINE} {DIM}{hint}{RESET}{}",
            " ".repeat((width as usize).saturating_sub(hint.chars().count() + 1))
        );
        print!("{RESET}");
        Ok(())
    }

    fn render_working_row(
        &self,
        width: u16,
        height: u16,
        composer_rows: u16,
        activity: &str,
        frame: &str,
    ) -> anyhow::Result<()> {
        let row = height.saturating_sub(composer_rows).max(1);
        let max_activity = (width as usize).saturating_sub(5).max(8);
        let activity = truncate_for_terminal(activity, max_activity);
        let body = format!("{WHITE}{BOLD}{frame}{RESET} {WHITE}{activity}{RESET}");
        let padding = (width as usize).saturating_sub(ansi_visible_width(&body) + 1);
        print!(
            "\x1b[{row};1H{RESET}{CLEAR_LINE} {body}{}",
            " ".repeat(padding)
        );
        print!("{RESET}");
        Ok(())
    }

    fn render_menu(
        &self,
        menu: Option<&CompletionMenu>,
        width: u16,
        height: u16,
        composer_rows: u16,
    ) -> anyhow::Result<u16> {
        let Some(menu) = menu else {
            return Ok(0);
        };
        let max_visible: usize = 8;
        let visible = menu.options.len().min(max_visible).max(1);
        let total_rows = (visible + 2) as u16; // +top border +bottom border
        let start = height
            .saturating_sub(total_rows.saturating_add(composer_rows).saturating_sub(1))
            .max(1);
        let title = match menu.mode {
            CompletionMode::Slash => "/ commands",
            CompletionMode::Mention => "@ files & agents",
        };

        // Compute scroll window so the selected row stays in view
        let scroll = if menu.options.len() <= visible {
            0
        } else if menu.selected < visible / 2 {
            0
        } else if menu.selected + visible / 2 >= menu.options.len() {
            menu.options.len() - visible
        } else {
            menu.selected - visible / 2
        };

        let inner_width = (width as usize).saturating_sub(2);
        let title_label = format!(" {title} ");
        let title_chars = ansi_visible_width(&title_label);
        let top_fill = inner_width.saturating_sub(title_chars + 1);
        let top_left = format!(
            "{DIM}{BORDER_TL}{BORDER_H}{RESET}{BOLD}{CYAN}{title_label}{RESET}{DIM}{}{BORDER_TR}{RESET}",
            BORDER_H.repeat(top_fill)
        );
        print!("\x1b[{start};1H{RESET}{CLEAR_LINE}{top_left}");

        // Body rows
        if menu.options.is_empty() {
            let row = start + 1;
            let body = format!("{RED} no matches{RESET}");
            let body_chars = ansi_visible_width(&body);
            let pad = inner_width.saturating_sub(body_chars);
            print!(
                "\x1b[{row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_V}{RESET}{body}{}{DIM}{BORDER_V}{RESET}",
                " ".repeat(pad)
            );
            // Empty rows to fill visible
            for r in 1..visible {
                let row = start + 1 + r as u16;
                let empty = " ".repeat(inner_width);
                print!(
                    "\x1b[{row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_V}{RESET}{empty}{DIM}{BORDER_V}{RESET}"
                );
            }
        } else {
            for r in 0..visible {
                let opt_idx = scroll + r;
                let row = start + 1 + r as u16;
                if opt_idx >= menu.options.len() {
                    let empty = " ".repeat(inner_width);
                    print!(
                        "\x1b[{row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_V}{RESET}{empty}{DIM}{BORDER_V}{RESET}"
                    );
                    continue;
                }
                let option = &menu.options[opt_idx];
                let is_selected = opt_idx == menu.selected;
                let name_width: usize = 24;
                let display = truncate_for_terminal(&option.display, name_width);
                let desc_width = inner_width.saturating_sub(4 + name_width + 1).max(4);
                let description = truncate_for_terminal(&option.description, desc_width);

                let body = if is_selected {
                    format!(
                        " {BOLD}{CYAN}▶{RESET} {BOLD}{CYAN}{display:<name_width$}{RESET} {DIM}{description:<desc_width$}{RESET} "
                    )
                } else {
                    format!(
                        "   {CYAN}{display:<name_width$}{RESET} {DIM}{description:<desc_width$}{RESET} "
                    )
                };

                print!(
                    "\x1b[{row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_V}{RESET}{body}{DIM}{BORDER_V}{RESET}"
                );
            }
        }

        // Bottom border with hint + count
        let hint = " ↑↓ navigate · enter accept · esc cancel ";
        let count = if menu.options.is_empty() {
            String::new()
        } else {
            format!(" {}/{} ", menu.selected + 1, menu.options.len())
        };
        let hint_chars = hint.chars().count();
        let count_chars = count.chars().count();
        let fill = inner_width
            .saturating_sub(hint_chars + count_chars + 2)
            .max(1);
        let bottom_row = start + total_rows - 1;
        print!(
            "\x1b[{bottom_row};1H{RESET}{CLEAR_LINE}{DIM}{BORDER_BL}{BORDER_H}{RESET}{DIM}{hint}{RESET}{DIM}{}{RESET}{DIM}{count}{BORDER_H}{BORDER_BR}{RESET}",
            BORDER_H.repeat(fill)
        );

        Ok(total_rows)
    }

    pub(crate) fn render_status(
        &mut self,
        activity: &str,
        input: &str,
    ) -> anyhow::Result<()> {
        self.render_status_with_menu(activity, input, None)
    }

    pub(crate) fn render_status_with_menu(
        &mut self,
        activity: &str,
        input: &str,
        menu: Option<&CompletionMenu>,
    ) -> anyhow::Result<()> {
        let (width, height) = terminal_size();
        print!("\x1b[?25l\x1b[s");
        self.clear_overlay()?;
        let composer_rows = self.composer_reserved_rows(width, input);
        let bottom_rows = composer_rows + WORKING_ROWS;
        let menu_rows = self.render_menu(menu, width, height, bottom_rows)?;
        let total_reserved = menu_rows + bottom_rows;
        self.set_scroll_region(total_reserved)?;
        let frame = SPINNER_FRAMES[self.spinner % SPINNER_FRAMES.len()];
        self.spinner = self.spinner.wrapping_add(1);
        let footer_row = height.max(1);
        self.render_working_row(width, height, composer_rows, activity, frame)?;
        self.draw_input_panel(width, height, input)?;
        self.render_footer(
            footer_row,
            width,
            "",
            FooterMode::Working { activity, frame },
        )?;
        print!("\x1b[u");
        std::io::stdout().flush()?;
        self.overlay_rows = total_reserved;
        Ok(())
    }

    pub(crate) fn before_output(&mut self) -> anyhow::Result<()> {
        let reserved_rows = self.overlay_rows.max(FOOTER_ROWS + 1);
        self.clear_overlay()?;
        let (_, height) = terminal_size();
        self.reset_scroll_region()?;
        let row = output_row_before_overlay(height, reserved_rows);
        print!("\x1b[{row};1H{RESET}{CLEAR_LINE}");
        std::io::stdout().flush()?;
        self.overlay_rows = 0;
        Ok(())
    }

    pub(crate) fn clear_overlay(&mut self) -> anyhow::Result<()> {
        if self.overlay_rows == 0 {
            return Ok(());
        }
        let (_, height) = terminal_size();
        let start = height.saturating_sub(self.overlay_rows).saturating_add(1);
        for row in start..=height {
            print!("\x1b[{row};1H{RESET}{CLEAR_LINE}");
        }
        std::io::stdout().flush()?;
        self.overlay_rows = 0;
        Ok(())
    }

    pub(crate) fn clear_overlay_preserving_cursor(&mut self) -> anyhow::Result<()> {
        if self.overlay_rows == 0 {
            return Ok(());
        }
        print!("\x1b[s");
        self.clear_overlay()?;
        print!("\x1b[u");
        std::io::stdout().flush()?;
        Ok(())
    }

    fn set_scroll_region(&self, reserved_rows: u16) -> anyhow::Result<()> {
        let (_, height) = terminal_size();
        let bottom = height.saturating_sub(reserved_rows).max(1);
        print!("\x1b[1;{bottom}r");
        Ok(())
    }

    fn reset_scroll_region(&self) -> anyhow::Result<()> {
        let (_, height) = terminal_size();
        print!("\x1b[1;{height}r");
        Ok(())
    }
}

pub(crate) fn print_user_prompt(text: &str) {
    let (width, _) = terminal_size();
    let available = (width as usize).saturating_sub(2);
    println!();
    for (index, line) in text.lines().enumerate() {
        let line = truncate_for_terminal(line, available);
        if index == 0 {
            println!("{BOLD}{CYAN}{PROMPT_MARKER}{RESET} {BOLD}{line}{RESET}");
        } else {
            println!("  {BOLD}{line}{RESET}");
        }
    }
    if text.is_empty() {
        println!("{BOLD}{CYAN}{PROMPT_MARKER}{RESET}");
    }
}

fn input_layout(width: u16, input: &str) -> InputLayout {
    let capacity = text_capacity(width).max(1);
    let mut rows = wrap_input_rows(input, capacity);
    if rows.is_empty() {
        rows.push(String::new());
    }
    if rows.len() > MAX_INPUT_ROWS {
        let start = rows.len() - MAX_INPUT_ROWS;
        rows = rows.split_off(start);
        if let Some(first) = rows.first_mut() {
            let keep = capacity.saturating_sub(3);
            let visible = first.chars().take(keep).collect::<String>();
            *first = format!("...{visible}");
        }
    }
    InputLayout { rows }
}

fn text_capacity(width: u16) -> usize {
    (width as usize).saturating_sub(3).max(1)
}

fn wrap_input_rows(input: &str, capacity: usize) -> Vec<String> {
    let mut rows = Vec::new();
    for raw_line in input.split('\n') {
        let mut current = String::new();
        let mut current_width = 0usize;
        for ch in raw_line.chars() {
            if current_width >= capacity {
                rows.push(current);
                current = String::new();
                current_width = 0;
            }
            current.push(ch);
            current_width += 1;
        }
        rows.push(current);
    }
    rows
}

fn output_row_before_overlay(height: u16, reserved_rows: u16) -> u16 {
    height.saturating_sub(reserved_rows).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_layout_wraps_long_prompt_lines() {
        let layout = input_layout(12, "abcdefghijklmnop");
        assert_eq!(layout.rows, vec!["abcdefghi", "jklmnop"]);
    }

    #[test]
    fn input_layout_caps_visible_rows_to_recent_content() {
        let layout = input_layout(8, "abcdefghijklmnopqrstuvwxyz");
        assert_eq!(layout.rows.len(), MAX_INPUT_ROWS);
        assert!(layout.rows[0].starts_with("..."));
    }

    #[test]
    fn output_row_stays_inside_scroll_region_above_overlay() {
        assert_eq!(output_row_before_overlay(40, 2), 38);
        assert_eq!(output_row_before_overlay(40, 4), 36);
    }
}
