//! Render markdown to a page-sized PDF the reMarkable can display, plus a
//! **layout map** so ink drawn over a line can be anchored back to the
//! source text.
//!
//! This is a deliberately small, dependency-free PDF writer: one built-in
//! font (Helvetica), text positioned line by line on `1404×1872` pages
//! (the reMarkable frame). It's not a full markdown renderer — headings
//! get a bigger size and inline markers are stripped for display — but it
//! produces a valid PDF and, crucially, the per-line `LayoutItem`s that
//! tie rendered position ↔ source offset. A richer renderer can replace
//! it later without changing the bundle/anchor plumbing.

use crate::{PAGE_HEIGHT, PAGE_WIDTH};

/// Generous margin so the text block clears the reMarkable's left tool
/// palette (and top bar) and reads centered, like a native page.
const MARGIN: f32 = 160.0;
const BODY_SIZE: f32 = 32.0;
const LINE_GAP: f32 = 1.4;
/// Helvetica is ~0.5em average advance; good enough for wrapping.
const CHAR_W: f32 = 0.5;

/// One rendered line: where it landed (top-left origin, same frame as ink
/// strokes) and which character of the source it started at — the hook
/// for anchoring handwriting to words.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutItem {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub text: String,
    pub source_offset: usize,
}

/// A rendered document: the PDF bytes, page count, and the layout map.
#[derive(Debug, Clone)]
pub struct RenderedPdf {
    pub bytes: Vec<u8>,
    pub page_count: usize,
    pub layout: Vec<LayoutItem>,
}

/// Render markdown into a paginated PDF + layout map.
pub fn markdown_to_pdf(markdown: &str) -> RenderedPdf {
    let max_y = PAGE_HEIGHT - MARGIN;
    let usable_w = PAGE_WIDTH - 2.0 * MARGIN;

    let mut layout: Vec<LayoutItem> = Vec::new();
    // page text ops in PDF space (bottom-left origin): (x, pdf_y, size, text)
    let mut pages: Vec<Vec<(f32, f32, f32, String)>> = vec![Vec::new()];
    let mut y = MARGIN; // top-left cursor
    let mut source_offset = 0usize;

    for raw_line in markdown.split('\n') {
        let line_chars = raw_line.chars().count() + 1; // +1 for the '\n'
        let (display, size) = strip_heading(raw_line);
        if display.is_empty() {
            y += BODY_SIZE * LINE_GAP * 0.6; // blank line: smaller gap
            source_offset += line_chars;
            continue;
        }
        let max_chars = ((usable_w / (size * CHAR_W)).floor() as usize).max(1);
        for wrapped in wrap(&display, max_chars) {
            if y + size > max_y {
                pages.push(Vec::new());
                y = MARGIN;
            }
            let page = pages.len() - 1;
            let pdf_y = PAGE_HEIGHT - y - size; // flip to PDF bottom-left origin
            pages[page].push((MARGIN, pdf_y, size, wrapped.clone()));
            layout.push(LayoutItem {
                page,
                x: MARGIN,
                y,
                size,
                text: wrapped,
                source_offset,
            });
            y += size * LINE_GAP;
        }
        source_offset += line_chars;
    }

    let bytes = build_pdf(&pages);
    RenderedPdf {
        page_count: pages.len(),
        bytes,
        layout,
    }
}

/// Strip a leading `#`/`##`/`###` and return (text, font size).
fn strip_heading(line: &str) -> (String, f32) {
    let t = line.trim_end();
    let hashes = t.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && t.as_bytes().get(hashes) == Some(&b' ') {
        let size = match hashes {
            1 => 56.0,
            2 => 46.0,
            3 => 40.0,
            _ => 36.0,
        };
        (strip_inline(t[hashes + 1..].trim()), size)
    } else {
        (strip_inline(t), BODY_SIZE)
    }
}

/// Drop the most common inline markers for display (the real text stays
/// canonical in Neoism; this is just what shows on the tablet page).
fn strip_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '`' => {}       // bold/italic/code marks
            '-' if out.is_empty() => {} // bullet dash
            '\\' => {
                if let Some(&n) = chars.peek() {
                    out.push(n);
                    chars.next();
                }
            }
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
        } else if cur.chars().count() + 1 + word.chars().count() <= max_chars {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Escape a string for a PDF literal `(...)` and drop non-WinAnsi bytes.
fn pdf_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            c if (c as u32) >= 0x20 && (c as u32) < 0x7f => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

/// Assemble a minimal valid PDF from per-page text ops (in PDF space).
fn build_pdf(pages: &[Vec<(f32, f32, f32, String)>]) -> Vec<u8> {
    const FONT_NUM: usize = 3;
    let mut numbered: Vec<(usize, Vec<u8>)> = Vec::new();
    numbered.push((1, b"<</Type/Catalog/Pages 2 0 R>>".to_vec()));
    numbered.push((
        3,
        b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica/Encoding/WinAnsiEncoding>>"
            .to_vec(),
    ));

    let mut kids = String::new();
    let mut num = 4;
    for page in pages {
        let (page_num, content_num) = (num, num + 1);
        num += 2;
        kids.push_str(&format!("{page_num} 0 R "));

        let mut stream = String::new();
        for (x, y, size, text) in page {
            stream.push_str(&format!(
                "BT /F1 {size:.0} Tf {x:.1} {y:.1} Td ({}) Tj ET\n",
                pdf_escape(text)
            ));
        }
        let content = format!("<</Length {}>>\nstream\n{stream}endstream", stream.len());
        numbered.push((content_num, content.into_bytes()));
        let page_obj = format!(
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 {w:.0} {h:.0}]\
             /Resources<</Font<</F1 {FONT_NUM} 0 R>>>>/Contents {content_num} 0 R>>",
            w = PAGE_WIDTH,
            h = PAGE_HEIGHT,
        );
        numbered.push((page_num, page_obj.into_bytes()));
    }
    numbered.push((
        2,
        format!("<</Type/Pages/Kids[{kids}]/Count {}>>", pages.len()).into_bytes(),
    ));
    numbered.sort_by_key(|(n, _)| *n);

    let count = numbered.len() + 1; // +1 for the free object 0
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize; count];
    for (n, body) in &numbered {
        offsets[*n] = out.len();
        out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_pos = out.len();
    out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for n in 1..count {
        out.extend_from_slice(format!("{:010} 00000 n \n", offsets[n]).as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<</Size {count}/Root 1 0 R>>\nstartxref\n{xref_pos}\n%%EOF")
            .as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_a_valid_pdf_and_layout() {
        let md = "# Title\n\nHello world, this is a note.\n\nSecond paragraph.";
        let r = markdown_to_pdf(md);
        assert!(r.bytes.starts_with(b"%PDF-1.4"));
        assert!(r.bytes.ends_with(b"%%EOF"));
        assert!(r.page_count >= 1);
        assert!(!r.layout.is_empty());
        // The heading should render bigger than body text.
        let title = &r.layout[0];
        assert_eq!(title.text, "Title");
        assert!(title.size > BODY_SIZE);
        // Layout y increases down the page.
        assert!(r.layout.last().unwrap().y > title.y);
    }

    #[test]
    fn long_text_paginates() {
        let md = "word ".repeat(4000);
        let r = markdown_to_pdf(&md);
        assert!(r.page_count > 1, "should overflow onto multiple pages");
    }
}
