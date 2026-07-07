use super::*;

pub(crate) fn notebook_output_marker_lines(
    lines: &[String],
    start: usize,
    end: usize,
) -> Vec<usize> {
    if lines.is_empty() || start >= lines.len() {
        return Vec::new();
    }
    let end = end.min(lines.len().saturating_sub(1));
    lines
        .iter()
        .enumerate()
        .take(end.saturating_add(1))
        .skip(start)
        .filter_map(|(line_ix, line)| {
            is_notebook_output_marker_line(line).then_some(line_ix)
        })
        .collect()
}

pub(crate) fn output_marker_line_count(text: &str) -> usize {
    let count = text.lines().count();
    if count == 0 {
        1
    } else {
        count
    }
}

pub(crate) fn output_display_id(output: &Value) -> Option<&str> {
    output
        .get("transient")
        .and_then(|transient| transient.get("display_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

pub(crate) fn append_outputs(
    markdown: &mut String,
    cell: &NotebookCell,
    elapsed_ms: Option<u128>,
    is_running: bool,
) {
    if cell.outputs.is_empty() {
        if let Some(elapsed_ms) = elapsed_ms {
            let prompt = if is_running { "In [*]" } else { "Done" };
            append_notebook_output_lines(
                markdown,
                prompt,
                Some(elapsed_ms),
                "",
                is_running,
            );
        }
        return;
    }
    for (idx, output) in cell.outputs.iter().enumerate() {
        if let Some(text) = output_text(output) {
            append_notebook_output_lines(
                markdown,
                &notebook_output_prompt(cell, output, is_running),
                if idx == 0 { elapsed_ms } else { None },
                &text,
                is_running,
            );
        }
    }
}

pub(crate) fn append_notebook_output_lines(
    markdown: &mut String,
    prompt: &str,
    elapsed_ms: Option<u128>,
    text: &str,
    is_running: bool,
) {
    let prompt = escape_notebook_meta(prompt);
    let elapsed = elapsed_ms
        .map(|elapsed_ms| format_elapsed(elapsed_ms).replace(' ', "_"))
        .unwrap_or_else(|| "_".to_string());
    let mut wrote = false;
    for (idx, line) in text.lines().enumerate() {
        markdown.push_str("%%neoism_notebook_output ");
        markdown.push_str(if idx == 0 { &prompt } else { "_" });
        markdown.push(' ');
        markdown.push_str(if idx == 0 { &elapsed } else { "_" });
        if is_running && idx == 0 {
            markdown.push_str(" neoism_state=running");
        }
        markdown.push(' ');
        markdown.push_str(line);
        markdown.push('\n');
        wrote = true;
    }
    if !wrote {
        markdown.push_str("%%neoism_notebook_output ");
        markdown.push_str(&prompt);
        markdown.push(' ');
        markdown.push_str(&elapsed);
        if is_running {
            markdown.push_str(" neoism_state=running");
        }
        markdown.push('\n');
    }
}

pub(crate) fn output_text(output: &Value) -> Option<String> {
    match output.get("output_type").and_then(Value::as_str) {
        Some("stream") => output_text_value(output.get("text")),
        Some("error") => error_output_text(output),
        Some("display_data") | Some("execute_result") => rich_output_text(output)
            .or_else(|| data_mime_text(output, "text/plain"))
            .or_else(|| output_text_value(output.get("text")))
            .or_else(|| error_output_text(output)),
        _ => output_text_value(output.get("text"))
            .or_else(|| rich_output_text(output))
            .or_else(|| error_output_text(output)),
    }
}

pub(crate) fn value_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(""),
        ),
        _ => None,
    }
}

pub(crate) fn output_text_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(sanitize_notebook_output_preview_text(text)),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("");
            Some(sanitize_notebook_output_preview_text(&text))
        }
        _ => None,
    }
}

pub(crate) fn rich_output_text(output: &Value) -> Option<String> {
    let data = output.get("data")?.as_object()?;
    let (mime, value) = preferred_rich_mime(data)?;
    render_mime_output(mime, value)
}

pub(crate) fn data_mime_text(output: &Value, mime: &str) -> Option<String> {
    output
        .get("data")
        .and_then(|data| data.get(mime))
        .and_then(|value| output_text_value(Some(value)))
}

pub(crate) fn error_output_text(output: &Value) -> Option<String> {
    output_text_value(output.get("traceback")).or_else(|| {
        output.get("ename").and_then(Value::as_str).map(|ename| {
            let evalue = output.get("evalue").and_then(Value::as_str).unwrap_or("");
            let text = if evalue.is_empty() {
                ename.to_string()
            } else {
                format!("{ename}: {evalue}")
            };
            sanitize_notebook_output_preview_text(&text)
        })
    })
}

pub(crate) fn preferred_rich_mime<'a>(
    data: &'a serde_json::Map<String, Value>,
) -> Option<(&'a str, &'a Value)> {
    const PRIORITY: &[&str] = &[
        "image/png",
        "image/jpeg",
        "image/webp",
        "image/gif",
        "image/svg+xml",
        "text/html",
        "text/latex",
        "application/vnd.plotly.v1+json",
        "application/vnd.vegalite.v5+json",
        "application/vnd.vegalite.v4+json",
        "application/vnd.vega.v5+json",
        "application/json",
        "text/markdown",
        "text/plain",
    ];
    for mime in PRIORITY {
        if let Some(value) = data.get(*mime) {
            return Some((*mime, value));
        }
    }
    data.iter()
        .find(|(mime, _)| mime.starts_with("image/"))
        .map(|(mime, value)| (mime.as_str(), value))
        .or_else(|| {
            data.iter()
                .next()
                .map(|(mime, value)| (mime.as_str(), value))
        })
}

pub(crate) fn render_mime_output(mime: &str, value: &Value) -> Option<String> {
    if mime.starts_with("image/") {
        return Some(image_output_summary(mime, value));
    }
    match mime {
        "text/plain" => output_text_value(Some(value)),
        "text/html" => render_html_output(value),
        "text/latex" => render_latex_output(value),
        "text/markdown" => output_text_value(Some(value)).map(|text| {
            if text.trim().is_empty() {
                "Markdown output".to_string()
            } else {
                format!("Markdown output:\n{text}")
            }
        }),
        "application/json"
        | "application/vnd.plotly.v1+json"
        | "application/vnd.vega.v5+json"
        | "application/vnd.vegalite.v4+json"
        | "application/vnd.vegalite.v5+json" => render_json_output(mime, value),
        _ => render_unknown_mime_output(mime, value),
    }
}

pub(crate) fn render_latex_output(value: &Value) -> Option<String> {
    let latex = output_text_value(Some(value))?;
    let latex = latex.trim();
    if latex.is_empty() {
        Some("LaTeX output".to_string())
    } else {
        Some(format!("LaTeX output:\n{latex}"))
    }
}

pub(crate) fn render_json_output(mime: &str, value: &Value) -> Option<String> {
    let json = serde_json::to_string_pretty(value).ok()?;
    let json = sanitize_notebook_output_preview_text(&json);
    let label = match mime {
        "application/json" => "JSON output",
        "application/vnd.plotly.v1+json" => "Plotly output",
        "application/vnd.vega.v5+json" => "Vega output",
        "application/vnd.vegalite.v4+json" | "application/vnd.vegalite.v5+json" => {
            "Vega-Lite output"
        }
        _ => "JSON output",
    };
    Some(format!("{label}:\n{json}"))
}

pub(crate) fn render_html_output(value: &Value) -> Option<String> {
    let html = value_text(Some(value))?;
    if let Some(table) = html_tables_to_markdown(&html) {
        let table = sanitize_notebook_output_preview_text(&table);
        return Some(format!("HTML table output:\n{table}"));
    }
    let text = sanitize_notebook_output_preview_text(&html_to_visible_text(&html));
    if text.trim().is_empty() {
        Some(format!("HTML output ({} chars)", html.chars().count()))
    } else {
        Some(format!("HTML output:\n{text}"))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HtmlTableCell {
    text: String,
    header: bool,
}

pub(crate) fn html_tables_to_markdown(html: &str) -> Option<String> {
    let tables = html_tag_blocks(html, "table")
        .into_iter()
        .filter_map(|table| html_table_to_markdown(table))
        .collect::<Vec<_>>();
    if tables.is_empty() {
        None
    } else {
        Some(tables.join("\n\n"))
    }
}

pub(crate) fn html_table_to_markdown(table: &str) -> Option<String> {
    let mut rows = html_tag_blocks(table, "tr")
        .into_iter()
        .map(|row| html_table_row_cells(row))
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>();
    let max_cols = rows.iter().map(Vec::len).max()?;
    if max_cols == 0 {
        return None;
    }
    for row in &mut rows {
        while row.len() < max_cols {
            row.push(HtmlTableCell {
                text: String::new(),
                header: false,
            });
        }
    }

    let first_row_is_header = rows
        .first()
        .is_some_and(|row| row.iter().any(|cell| cell.header));
    let mut out = String::new();
    if first_row_is_header {
        out.push_str(&markdown_table_row(
            rows[0].iter().map(|cell| cell.text.as_str()),
        ));
        out.push('\n');
        out.push_str(&markdown_table_separator(max_cols));
        for row in rows.iter().skip(1) {
            out.push('\n');
            out.push_str(&markdown_table_row(
                row.iter().map(|cell| cell.text.as_str()),
            ));
        }
    } else {
        let headers = (1..=max_cols)
            .map(|idx| format!("Column {idx}"))
            .collect::<Vec<_>>();
        out.push_str(&markdown_table_row(headers.iter().map(String::as_str)));
        out.push('\n');
        out.push_str(&markdown_table_separator(max_cols));
        for row in &rows {
            out.push('\n');
            out.push_str(&markdown_table_row(
                row.iter().map(|cell| cell.text.as_str()),
            ));
        }
    }
    Some(out)
}

pub(crate) fn html_table_row_cells(row: &str) -> Vec<HtmlTableCell> {
    let lower = row.to_ascii_lowercase();
    let mut cells = Vec::new();
    let mut search = 0usize;
    while search < row.len() {
        let td = lower[search..].find("<td").map(|idx| (search + idx, "td"));
        let th = lower[search..].find("<th").map(|idx| (search + idx, "th"));
        let Some((open, tag)) =
            [td, th].into_iter().flatten().min_by_key(|(idx, _)| *idx)
        else {
            break;
        };
        let Some(open_end_rel) = lower[open..].find('>') else {
            break;
        };
        let content_start = open + open_end_rel + 1;
        let close_pat = format!("</{tag}>");
        let Some(close_rel) = lower[content_start..].find(&close_pat) else {
            search = content_start;
            continue;
        };
        let close = content_start + close_rel;
        let text =
            html_table_cell_text(&html_to_visible_text(&row[content_start..close]));
        cells.push(HtmlTableCell {
            text,
            header: tag == "th",
        });
        search = close + close_pat.len();
    }
    cells
}

pub(crate) fn html_tag_blocks<'a>(html: &'a str, tag: &str) -> Vec<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}>");
    let mut blocks = Vec::new();
    let mut search = 0usize;
    while search < html.len() {
        let Some(open_rel) = lower[search..].find(&open_pat) else {
            break;
        };
        let open = search + open_rel;
        let Some(open_end_rel) = lower[open..].find('>') else {
            break;
        };
        let content_start = open + open_end_rel + 1;
        let Some(close_rel) = lower[content_start..].find(&close_pat) else {
            break;
        };
        let close = content_start + close_rel;
        blocks.push(&html[content_start..close]);
        search = close + close_pat.len();
    }
    blocks
}

pub(crate) fn markdown_table_row<'a>(cells: impl IntoIterator<Item = &'a str>) -> String {
    let cells = cells
        .into_iter()
        .map(|cell| markdown_table_cell_text(cell))
        .collect::<Vec<_>>();
    format!("| {} |", cells.join(" | "))
}

pub(crate) fn markdown_table_separator(cols: usize) -> String {
    format!(
        "| {} |",
        std::iter::repeat("---")
            .take(cols)
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

pub(crate) fn markdown_table_cell_text(text: &str) -> String {
    html_table_cell_text(text).replace('|', "\\|")
}

pub(crate) fn html_table_cell_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn render_unknown_mime_output(mime: &str, value: &Value) -> Option<String> {
    output_text_value(Some(value))
        .filter(|text| !text.trim().is_empty())
        .map(|text| format!("{mime} output:\n{text}"))
        .or_else(|| {
            serde_json::to_string_pretty(value)
                .ok()
                .map(|json| sanitize_notebook_output_preview_text(&json))
                .map(|json| format!("{mime} output:\n{json}"))
        })
}

pub(crate) fn html_to_visible_text(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let mut entity = String::new();
    let mut in_entity = false;
    for ch in html.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
            continue;
        }
        if in_entity {
            if ch == ';' {
                out.push_str(match entity.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" | "#39" => "'",
                    "nbsp" => " ",
                    _ => "",
                });
                entity.clear();
                in_entity = false;
            } else if entity.len() < 16 {
                entity.push(ch);
            } else {
                entity.clear();
                in_entity = false;
            }
            continue;
        }
        match ch {
            '<' => in_tag = true,
            '&' => in_entity = true,
            _ => out.push(ch),
        }
    }
    out.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn notebook_output_prompt(
    cell: &NotebookCell,
    output: &Value,
    is_running: bool,
) -> String {
    let count = cell
        .execution_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| {
            if is_running {
                "*".to_string()
            } else {
                " ".to_string()
            }
        });
    match output.get("output_type").and_then(Value::as_str) {
        Some("stream") => match output.get("name").and_then(Value::as_str) {
            Some("stderr") => format!("Err [{count}]"),
            Some("stdout") | None => String::new(),
            Some(name) => format!("{name} [{count}]"),
        },
        Some("error") => format!("Err [{count}]"),
        Some("execute_result") | Some("display_data") => format!("Out [{count}]"),
        Some(other) => format!("{other} [{count}]"),
        None => format!("Out [{count}]"),
    }
}

pub(crate) fn escape_notebook_meta(value: &str) -> String {
    if value.is_empty() {
        "_".to_string()
    } else {
        value.replace(' ', "_")
    }
}
