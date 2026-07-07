use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedTaskUpdate {
    pub path: PathBuf,
    pub line: usize,
    pub checked: bool,
}

pub fn generated_task_source_marker(path: &Path, line: i64) -> String {
    format!(
        "<!-- neoism-task:{}:{} -->",
        hex_encode(path.display().to_string().as_bytes()),
        line.max(1)
    )
}

pub fn parse_generated_task_update(line: &str) -> Option<GeneratedTaskUpdate> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("- [")?;
    let marker = rest.chars().next()?;
    if !matches!(marker, ' ' | 'x' | 'X') {
        return None;
    }
    let rest = rest.get(marker.len_utf8()..)?;
    let rest = rest.strip_prefix(']')?;
    if let Some((path, line)) = parse_generated_task_source_marker(rest) {
        return Some(GeneratedTaskUpdate {
            path,
            line,
            checked: matches!(marker, 'x' | 'X'),
        });
    }
    let mut search_from = 0usize;
    let mut found = None;
    while let Some(start_rel) = rest.get(search_from..)?.find("[[") {
        let start = search_from + start_rel + 2;
        let Some(end_rel) = rest.get(start..)?.find("]]") else {
            break;
        };
        let end = end_rel + start;
        let inner = rest.get(start..end)?;
        if let Some((path, line)) = parse_generated_task_source_link_inner(inner) {
            found = Some((path, line));
        }
        search_from = end + 2;
    }
    let (path, line) = found?;
    Some(GeneratedTaskUpdate {
        path,
        line,
        checked: matches!(marker, 'x' | 'X'),
    })
}

pub fn set_task_line_checked(line: &mut String, checked: bool) -> bool {
    let indent = line.len().saturating_sub(line.trim_start().len());
    let Some(rest) = line.get(indent..) else {
        return false;
    };
    let mut chars = rest.chars();
    let Some(bullet) = chars.next() else {
        return false;
    };
    if !matches!(bullet, '-' | '*' | '+') {
        return false;
    }
    let Some(rest) = chars.as_str().strip_prefix(" [") else {
        return false;
    };
    let Some(marker) = rest.chars().next() else {
        return false;
    };
    if !matches!(marker, ' ' | 'x' | 'X') {
        return false;
    }
    if !rest
        .get(marker.len_utf8()..)
        .is_some_and(|suffix| suffix.starts_with(']'))
    {
        return false;
    }
    let marker_ix = indent + bullet.len_utf8() + 2;
    let next = if checked { "x" } else { " " };
    if line.get(marker_ix..marker_ix + marker.len_utf8()) == Some(next) {
        return false;
    }
    line.replace_range(marker_ix..marker_ix + marker.len_utf8(), next);
    true
}

fn parse_generated_task_source_marker(text: &str) -> Option<(PathBuf, usize)> {
    let start = text.find("<!-- neoism-task:")? + "<!-- neoism-task:".len();
    let end = text.get(start..)?.find("-->")? + start;
    let payload = text.get(start..end)?.trim();
    let (path_hex, line) = payload.rsplit_once(':')?;
    let path = String::from_utf8(hex_decode(path_hex.trim())?).ok()?;
    let line = line.trim().parse::<usize>().ok()?.max(1);
    Some((PathBuf::from(path), line))
}

fn parse_generated_task_source_link_inner(inner: &str) -> Option<(PathBuf, usize)> {
    let target_part = inner
        .trim()
        .split_once('|')
        .map(|(target, _alias)| target.trim())
        .unwrap_or_else(|| inner.trim());
    let target = target_part
        .trim_start()
        .strip_prefix('@')
        .map(str::trim)
        .unwrap_or(target_part);
    if target.is_empty() || target.contains('#') {
        return None;
    }
    let (path, line) = target.rsplit_once('-')?;
    if path.trim().is_empty() || line.is_empty() {
        return None;
    }
    if !line.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return None;
    }
    Some((path, line.parse::<usize>().ok()?.max(1)))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    let mut bytes = value.bytes();
    while let (Some(high), Some(low)) = (bytes.next(), bytes.next()) {
        out.push((hex_digit(high)? << 4) | hex_digit(low)?);
    }
    Some(out)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
