use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;

use super::paths::{display_path, is_ignored_dir, truncate_line, wildcard_match};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GrepMatch {
    pub(super) path: String,
    pub(super) line: usize,
    pub(super) text: String,
    pub(super) mtime: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GlobMatch {
    pub(super) path: String,
    pub(super) mtime: u128,
}

pub(super) fn collect_grep_matches(
    cwd: &Path,
    root: &Path,
    pattern: &str,
    include: Option<&str>,
    exclude: Option<&str>,
) -> anyhow::Result<Vec<GrepMatch>> {
    if let Some(matches) = collect_grep_matches_rg(cwd, root, pattern, include, exclude) {
        return Ok(sort_grep_matches(matches));
    }
    let mut matches = Vec::new();
    collect_grep_matches_inner(cwd, root, pattern, include, exclude, &mut matches)?;
    Ok(sort_grep_matches(matches))
}

fn sort_grep_matches(mut matches: Vec<GrepMatch>) -> Vec<GrepMatch> {
    matches.sort_by(|a, b| {
        b.mtime
            .cmp(&a.mtime)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.cmp(&b.line))
    });
    matches
}

fn collect_grep_matches_rg(
    cwd: &Path,
    root: &Path,
    pattern: &str,
    include: Option<&str>,
    exclude: Option<&str>,
) -> Option<Vec<GrepMatch>> {
    let mut command = Command::new("rg");
    command
        .arg("--json")
        .arg("--color")
        .arg("never")
        .arg("--line-number");
    if let Some(include) = include {
        command.arg("--glob").arg(include);
    }
    if let Some(exclude) = exclude {
        command.arg("--glob").arg(negated_glob(exclude));
    }
    command.arg(pattern).arg(root);
    let output = command.output().ok()?;
    if !output.status.success() && output.status.code() != Some(1) {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut matches = Vec::new();
    for line in stdout.lines() {
        let value: Value = serde_json::from_str(line).ok()?;
        if value.get("type").and_then(Value::as_str) != Some("match") {
            continue;
        }
        let data = value.get("data")?;
        let raw_path = data
            .get("path")
            .and_then(|path| path.get("text"))
            .and_then(Value::as_str)?;
        let absolute = absolutize_rg_path(cwd, root, raw_path);
        let line_number = data.get("line_number").and_then(Value::as_u64)? as usize;
        let text = data
            .get("lines")
            .and_then(|lines| lines.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim_end_matches(['\r', '\n']);
        matches.push(GrepMatch {
            path: display_path(cwd, &absolute),
            line: line_number,
            text: truncate_line(text),
            mtime: mtime_ms(&absolute),
        });
    }
    Some(matches)
}

fn collect_grep_matches_inner(
    cwd: &Path,
    path: &Path,
    pattern: &str,
    include: Option<&str>,
    exclude: Option<&str>,
    matches: &mut Vec<GrepMatch>,
) -> anyhow::Result<()> {
    if path.is_dir() {
        let mut entries =
            std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            if is_ignored_dir(&entry.path()) {
                continue;
            }
            collect_grep_matches_inner(
                cwd,
                &entry.path(),
                pattern,
                include,
                exclude,
                matches,
            )?;
        }
        return Ok(());
    }

    if exclude
        .map(|pattern| matches_file_pattern(cwd, path, pattern))
        .unwrap_or(false)
    {
        return Ok(());
    }
    if include
        .map(|pattern| !matches_file_pattern(cwd, path, pattern))
        .unwrap_or(false)
    {
        return Ok(());
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(());
    };
    if bytes.contains(&0) {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&bytes);
    let mtime = mtime_ms(path);
    for (index, line) in text.lines().enumerate() {
        if line.contains(pattern) {
            matches.push(GrepMatch {
                path: display_path(cwd, path),
                line: index + 1,
                text: truncate_line(line),
                mtime,
            });
        }
    }
    Ok(())
}

pub(super) fn collect_glob_matches(
    cwd: &Path,
    root: &Path,
    pattern: &str,
    exclude: Option<&str>,
) -> anyhow::Result<Vec<GlobMatch>> {
    if let Some(matches) = collect_glob_matches_rg(cwd, root, pattern, exclude) {
        return Ok(sort_glob_matches(matches));
    }
    let mut matches = Vec::new();
    collect_glob_matches_inner(cwd, root, pattern, exclude, &mut matches)?;
    Ok(sort_glob_matches(matches))
}

fn sort_glob_matches(mut matches: Vec<GlobMatch>) -> Vec<GlobMatch> {
    matches.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.path.cmp(&b.path)));
    matches
}

fn collect_glob_matches_rg(
    cwd: &Path,
    root: &Path,
    pattern: &str,
    exclude: Option<&str>,
) -> Option<Vec<GlobMatch>> {
    let mut command = Command::new("rg");
    command.arg("--files").arg("--glob").arg(pattern);
    if let Some(exclude) = exclude {
        command.arg("--glob").arg(negated_glob(exclude));
    }
    command.arg(root);
    let output = command.output().ok()?;
    if !output.status.success() && output.status.code() != Some(1) {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(
        stdout
            .lines()
            .map(|line| {
                let absolute = absolutize_rg_path(cwd, root, line);
                GlobMatch {
                    path: display_path(cwd, &absolute),
                    mtime: mtime_ms(&absolute),
                }
            })
            .collect(),
    )
}

fn collect_glob_matches_inner(
    cwd: &Path,
    path: &Path,
    pattern: &str,
    exclude: Option<&str>,
    matches: &mut Vec<GlobMatch>,
) -> anyhow::Result<()> {
    if path.is_dir() {
        let mut entries =
            std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            if is_ignored_dir(&entry.path()) {
                continue;
            }
            collect_glob_matches_inner(cwd, &entry.path(), pattern, exclude, matches)?;
        }
        return Ok(());
    }

    if exclude
        .map(|pattern| matches_file_pattern(cwd, path, pattern))
        .unwrap_or(false)
    {
        return Ok(());
    }
    if matches_file_pattern(cwd, path, pattern) {
        matches.push(GlobMatch {
            path: display_path(cwd, path),
            mtime: mtime_ms(path),
        });
    }
    Ok(())
}

fn matches_file_pattern(cwd: &Path, path: &Path, pattern: &str) -> bool {
    let relative = display_path(cwd, path);
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    expand_braces(pattern).into_iter().any(|pattern| {
        wildcard_match(&pattern, &relative)
            || wildcard_match(&pattern, filename)
            || wildcard_match(&format!("**/{pattern}"), &relative)
    })
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let Some(close_offset) = pattern[open + 1..].find('}') else {
        return vec![pattern.to_string()];
    };
    let close = open + 1 + close_offset;
    let prefix = &pattern[..open];
    let suffix = &pattern[close + 1..];
    pattern[open + 1..close]
        .split(',')
        .map(|item| format!("{prefix}{item}{suffix}"))
        .collect()
}

fn negated_glob(pattern: &str) -> String {
    if pattern.starts_with('!') {
        pattern.to_string()
    } else {
        format!("!{pattern}")
    }
}

fn absolutize_rg_path(cwd: &Path, root: &Path, raw: &str) -> PathBuf {
    let raw_path = PathBuf::from(raw);
    if raw_path.is_absolute() {
        raw_path
    } else if root.is_dir() {
        root.join(raw_path)
    } else {
        cwd.join(raw_path)
    }
}

fn mtime_ms(path: &Path) -> u128 {
    std::fs::metadata(PathBuf::from(path))
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
