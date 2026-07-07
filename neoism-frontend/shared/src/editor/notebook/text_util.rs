use super::*;

pub(crate) fn sanitize_notebook_output_text(text: &str) -> String {
    normalize_notebook_carriage_returns(&strip_ansi_escapes(text))
}

pub(crate) fn sanitize_notebook_output_preview_text(text: &str) -> String {
    let (preview, truncated) = notebook_output_preview_slice(text);
    let mut output = sanitize_notebook_output_text(preview);
    if truncated {
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!(
            "[Neoism output truncated for display: showing {} of {} / max {} lines. Full output is preserved in notebook data.]",
            format_bytes(preview.len()),
            format_bytes(text.len()),
            NOTEBOOK_OUTPUT_DISPLAY_MAX_LINES
        ));
    }
    output
}

pub(crate) fn notebook_output_preview_slice(text: &str) -> (&str, bool) {
    let byte_end = notebook_output_preview_byte_end(text);
    let mut end = byte_end;
    let mut lines = 0usize;
    for (idx, ch) in text[..byte_end].char_indices() {
        if ch == '\n' {
            lines = lines.saturating_add(1);
            if lines >= NOTEBOOK_OUTPUT_DISPLAY_MAX_LINES {
                end = idx + ch.len_utf8();
                break;
            }
        }
    }
    (&text[..end], end < text.len())
}

pub(crate) fn notebook_output_preview_byte_end(text: &str) -> usize {
    if text.len() <= NOTEBOOK_OUTPUT_DISPLAY_MAX_BYTES {
        return text.len();
    }
    let mut end = NOTEBOOK_OUTPUT_DISPLAY_MAX_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

pub(crate) fn strip_ansi_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.next() {
                Some('[') => skip_csi_escape(&mut chars),
                Some(']') | Some('P') | Some('^') | Some('_') | Some('X') => {
                    skip_string_escape(&mut chars)
                }
                Some(_) | None => {}
            }
            continue;
        }
        if ch == '\u{9b}' {
            skip_csi_escape(&mut chars);
            continue;
        }
        out.push(ch);
    }
    out
}

pub(crate) fn skip_csi_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for ch in chars.by_ref() {
        if ('\u{40}'..='\u{7e}').contains(&ch) {
            break;
        }
    }
}

pub(crate) fn skip_string_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let mut saw_escape = false;
    for ch in chars.by_ref() {
        if saw_escape {
            if ch == '\\' {
                break;
            }
            saw_escape = false;
            continue;
        }
        if ch == '\u{7}' {
            break;
        }
        if ch == '\u{1b}' {
            saw_escape = true;
        }
    }
}

pub(crate) fn normalize_notebook_carriage_returns(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut line = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                out.push_str(&line);
                out.push('\n');
                line.clear();
            }
            '\r' => line.clear(),
            '\n' => {
                out.push_str(&line);
                out.push('\n');
                line.clear();
            }
            '\u{8}' => {
                line.pop();
            }
            _ => line.push(ch),
        }
    }
    out.push_str(&line);
    out
}

pub(crate) fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub(crate) fn format_elapsed(elapsed_ms: u128) -> String {
    if elapsed_ms < 1_000 {
        format!("{elapsed_ms} ms")
    } else {
        format!("{:.1} s", elapsed_ms as f64 / 1_000.0)
    }
}

pub(crate) fn ensure_trailing_newline(text: &mut String) {
    if !text.ends_with('\n') {
        text.push('\n');
    }
}
