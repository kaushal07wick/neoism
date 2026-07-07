//! Shared syntax highlighter for previews, markdown code blocks, diff
//! cards, finder cards, and agent chat code blocks.
//!
//! Native builds use tree-sitter where we have parser crates. Wasm and
//! unsupported languages keep the lightweight scanner fallback so UI code
//! can call one stable `highlight_line` entrypoint everywhere.
//!
//! Multi-line constructs (block comments, raw strings) are NOT carried
//! across lines — for a preview pane this trades correctness for
//! simplicity and the few cases where it misclassifies a line are
//! invisible in practice (the surrounding lines look right).

use crate::primitives::IdeTheme;

#[cfg(not(target_arch = "wasm32"))]
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
};

#[cfg(not(target_arch = "wasm32"))]
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

#[cfg(not(target_arch = "wasm32"))]
const TREE_SITTER_LINE_CACHE_LIMIT: usize = 4096;

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static TREE_SITTER_CACHE: RefCell<TreeSitterCache> = RefCell::new(TreeSitterCache::new());
}

#[cfg(not(target_arch = "wasm32"))]
const TREE_SITTER_HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "character",
    "character.special",
    "comment",
    "comment.documentation",
    "comment.error",
    "comment.note",
    "comment.todo",
    "comment.warning",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "escape",
    "field",
    "function",
    "function.builtin",
    "function.call",
    "function.method",
    "function.method.call",
    "keyword",
    "keyword.conditional",
    "keyword.conditional.ternary",
    "keyword.coroutine",
    "keyword.directive",
    "keyword.exception",
    "keyword.export",
    "keyword.function",
    "keyword.import",
    "keyword.modifier",
    "keyword.operator",
    "keyword.repeat",
    "keyword.return",
    "keyword.type",
    "module",
    "module.builtin",
    "namespace",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.escape",
    "string.regexp",
    "string.special",
    "symbol",
    "tag",
    "tag.attribute",
    "tag.delimiter",
    "type",
    "type.builtin",
    "type.definition",
    "annotation",
    "variable",
    "variable.builtin",
    "variable.member",
    "variable.parameter",
    "variable.super",
];

/// One coloured span of a preview line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SynTok {
    Plain,
    Keyword,
    Type,
    String,
    Number,
    Comment,
    Function,
    Punct,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ParserLang {
    Rust,
    Javascript,
    Jsx,
    Typescript,
    Tsx,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct HighlightCacheKey {
    lang: ParserLang,
    line: String,
}

#[cfg(not(target_arch = "wasm32"))]
struct TreeSitterCache {
    configs: HashMap<ParserLang, HighlightConfiguration>,
    lines: HashMap<HighlightCacheKey, Vec<(SynTok, usize, usize)>>,
    order: VecDeque<HighlightCacheKey>,
}

#[cfg(not(target_arch = "wasm32"))]
impl TreeSitterCache {
    fn new() -> Self {
        Self {
            configs: HashMap::new(),
            lines: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn config(&mut self, lang: ParserLang) -> Option<&HighlightConfiguration> {
        if !self.configs.contains_key(&lang) {
            let mut config = tree_sitter_config(lang)?;
            config.configure(TREE_SITTER_HIGHLIGHT_NAMES);
            self.configs.insert(lang, config);
        }
        self.configs.get(&lang)
    }

    fn line(&self, key: &HighlightCacheKey) -> Option<Vec<(SynTok, usize, usize)>> {
        self.lines.get(key).cloned()
    }

    fn insert_line(
        &mut self,
        key: HighlightCacheKey,
        spans: Vec<(SynTok, usize, usize)>,
    ) {
        if self.lines.contains_key(&key) {
            self.lines.insert(key, spans);
            return;
        }
        self.order.push_back(key.clone());
        self.lines.insert(key, spans);
        while self.order.len() > TREE_SITTER_LINE_CACHE_LIMIT {
            if let Some(old) = self.order.pop_front() {
                self.lines.remove(&old);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Javascript,
    Jsx,
    Typescript,
    Tsx,
    Python,
    Go,
    Lua,
    Toml,
    Json,
    /// Markdown source. The file-viewer in chrome.rs branches on this
    /// to render block-aware markdown (headings, lists, code blocks,
    /// quotes) via the lifted `editor::markdown::MarkdownPane` parser
    /// instead of the plain per-line syntax highlighter.
    Markdown,
    Other,
}

impl Lang {
    pub fn from_path(path: &str) -> Self {
        let ext = path
            .rsplit('.')
            .next()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        match ext.as_str() {
            "rs" => Lang::Rust,
            "js" | "mjs" | "cjs" => Lang::Javascript,
            "jsx" => Lang::Jsx,
            "ts" => Lang::Typescript,
            "tsx" => Lang::Tsx,
            "py" => Lang::Python,
            "go" => Lang::Go,
            "lua" => Lang::Lua,
            "toml" => Lang::Toml,
            "json" | "jsonc" => Lang::Json,
            "md" | "markdown" => Lang::Markdown,
            _ => Lang::Other,
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn",
                "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
                "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
                "self", "Self", "static", "struct", "super", "trait", "true", "type",
                "unsafe", "use", "where", "while", "yield",
            ],
            Lang::Javascript | Lang::Jsx | Lang::Typescript | Lang::Tsx => &[
                "async",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "export",
                "extends",
                "false",
                "finally",
                "for",
                "from",
                "function",
                "if",
                "import",
                "in",
                "instanceof",
                "let",
                "new",
                "null",
                "of",
                "return",
                "super",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "undefined",
                "var",
                "void",
                "while",
                "with",
                "yield",
                "interface",
                "type",
                "implements",
                "readonly",
                "abstract",
                "as",
                "namespace",
            ],
            Lang::Python => &[
                "False", "None", "True", "and", "as", "assert", "async", "await",
                "break", "class", "continue", "def", "del", "elif", "else", "except",
                "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
                "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
                "with", "yield",
            ],
            Lang::Go => &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
                "true",
                "false",
                "nil",
            ],
            Lang::Lua => &[
                "and", "break", "do", "else", "elseif", "end", "false", "for",
                "function", "goto", "if", "in", "local", "nil", "not", "or", "repeat",
                "return", "then", "true", "until", "while",
            ],
            Lang::Toml | Lang::Json | Lang::Markdown | Lang::Other => &[],
        }
    }

    fn type_starts_uppercase(self) -> bool {
        matches!(
            self,
            Lang::Rust
                | Lang::Javascript
                | Lang::Jsx
                | Lang::Typescript
                | Lang::Tsx
                | Lang::Go
        )
    }

    fn line_comment(self) -> Option<&'static str> {
        match self {
            Lang::Rust
            | Lang::Javascript
            | Lang::Jsx
            | Lang::Typescript
            | Lang::Tsx
            | Lang::Go => Some("//"),
            Lang::Python | Lang::Toml => Some("#"),
            Lang::Lua => Some("--"),
            Lang::Json | Lang::Markdown | Lang::Other => None,
        }
    }
}

pub fn highlight_line<'a>(line: &'a str, lang: Lang) -> Vec<(SynTok, &'a str)> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(spans) = tree_sitter_highlight_line(line, lang) {
        return spans;
    }

    fallback_highlight_line(line, lang)
}

fn fallback_highlight_line<'a>(line: &'a str, lang: Lang) -> Vec<(SynTok, &'a str)> {
    let bytes = line.as_bytes();
    let mut out: Vec<(SynTok, &'a str)> = Vec::new();
    let mut i = 0;

    let push = |out: &mut Vec<(SynTok, &'a str)>, kind: SynTok, slice: &'a str| {
        if !slice.is_empty() {
            out.push((kind, slice));
        }
    };

    let comment_marker = lang.line_comment();
    let kws = lang.keywords();

    while i < bytes.len() {
        let c = bytes[i];

        if let Some(marker) = comment_marker {
            let mb = marker.as_bytes();
            if bytes[i..].starts_with(mb) {
                push(&mut out, SynTok::Comment, &line[i..]);
                return out;
            }
        }

        if c == b'"' || c == b'\'' || c == b'`' {
            let start = i;
            let quote = c;
            i += 1;
            while i < bytes.len() {
                let bc = bytes[i];
                if bc == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if bc == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            push(&mut out, SynTok::String, &line[start..i]);
            continue;
        }

        if c.is_ascii_digit()
            || (c == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == b'.'
                    || bytes[i] == b'_')
            {
                i += 1;
            }
            push(&mut out, SynTok::Number, &line[start..i]);
            continue;
        }

        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            let slice = &line[start..i];
            let kind = if kws.contains(&slice) {
                SynTok::Keyword
            } else if lang.type_starts_uppercase()
                && slice
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false)
            {
                SynTok::Type
            } else if i < bytes.len() && bytes[i] == b'(' {
                SynTok::Function
            } else {
                SynTok::Plain
            };
            push(&mut out, kind, slice);
            continue;
        }

        if !c.is_ascii_alphanumeric() && c != b' ' && c != b'\t' {
            let start = i;
            while i < bytes.len() {
                let bc = bytes[i];
                if bc.is_ascii_alphanumeric()
                    || bc == b'_'
                    || bc == b' '
                    || bc == b'\t'
                    || bc == b'"'
                    || bc == b'\''
                    || bc == b'`'
                {
                    break;
                }
                if let Some(m) = comment_marker {
                    if bytes[i..].starts_with(m.as_bytes()) {
                        break;
                    }
                }
                i += 1;
            }
            push(&mut out, SynTok::Punct, &line[start..i]);
            continue;
        }

        let start = i;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i == start {
            i += 1;
        }
        push(&mut out, SynTok::Plain, &line[start..i]);
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn tree_sitter_highlight_line<'a>(
    line: &'a str,
    lang: Lang,
) -> Option<Vec<(SynTok, &'a str)>> {
    let parser_lang = ParserLang::from_lang(lang)?;
    let key = HighlightCacheKey {
        lang: parser_lang,
        line: line.to_owned(),
    };
    let cached = TREE_SITTER_CACHE.with(|cache| cache.borrow().line(&key));
    if let Some(spans) = cached {
        return Some(spans_to_slices(line, &spans));
    }

    let spans = TREE_SITTER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let config = cache.config(parser_lang)?;
        tree_sitter_highlight_ranges(line, lang, config)
    })?;

    TREE_SITTER_CACHE.with(|cache| cache.borrow_mut().insert_line(key, spans.clone()));
    Some(spans_to_slices(line, &spans))
}

#[cfg(not(target_arch = "wasm32"))]
impl ParserLang {
    fn from_lang(lang: Lang) -> Option<Self> {
        match lang {
            Lang::Rust => Some(ParserLang::Rust),
            Lang::Javascript => Some(ParserLang::Javascript),
            Lang::Jsx => Some(ParserLang::Jsx),
            Lang::Typescript => Some(ParserLang::Typescript),
            Lang::Tsx => Some(ParserLang::Tsx),
            _ => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spans_to_slices<'a>(
    line: &'a str,
    spans: &[(SynTok, usize, usize)],
) -> Vec<(SynTok, &'a str)> {
    spans
        .iter()
        .filter_map(|(kind, start, end)| {
            line.get(*start..*end).map(|slice| (*kind, slice))
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn tree_sitter_highlight_ranges(
    line: &str,
    lang: Lang,
    config: &HighlightConfiguration,
) -> Option<Vec<(SynTok, usize, usize)>> {
    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(&config, line.as_bytes(), None, |_| None)
        .ok()?;

    let mut out: Vec<(SynTok, usize, usize)> = Vec::new();
    let mut active: Vec<SynTok> = Vec::new();
    let mut had_highlight = false;

    for event in events {
        match event.ok()? {
            HighlightEvent::Source { start, end } => {
                if start >= end {
                    continue;
                }
                if let Some(kind) = active.last().copied() {
                    push_range(&mut out, kind, start, end);
                } else {
                    let fallback = fallback_highlight_line(&line[start..end], lang);
                    had_highlight |=
                        fallback.iter().any(|(tok, _)| *tok != SynTok::Plain);
                    out.extend(fallback.into_iter().map(|(kind, slice)| {
                        let slice_start =
                            slice.as_ptr() as usize - line.as_ptr() as usize;
                        (kind, slice_start, slice_start + slice.len())
                    }));
                }
            }
            HighlightEvent::HighlightStart(highlight) => {
                had_highlight = true;
                let kind = TREE_SITTER_HIGHLIGHT_NAMES
                    .get(highlight.0)
                    .copied()
                    .map(tree_sitter_capture_kind)
                    .unwrap_or(SynTok::Plain);
                active.push(kind);
            }
            HighlightEvent::HighlightEnd => {
                active.pop();
            }
        }
    }

    if had_highlight {
        Some(out)
    } else {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tree_sitter_config(lang: ParserLang) -> Option<HighlightConfiguration> {
    match lang {
        ParserLang::Rust => HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )
        .ok(),
        ParserLang::Javascript => HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        )
        .ok(),
        ParserLang::Jsx => HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            &format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
            ),
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        )
        .ok(),
        ParserLang::Typescript => HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            &format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_typescript::LOCALS_QUERY,
        )
        .ok(),
        ParserLang::Tsx => HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            &format!(
                "{}\n{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_typescript::LOCALS_QUERY,
        )
        .ok(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tree_sitter_capture_kind(capture: &str) -> SynTok {
    match capture {
        "comment"
        | "comment.documentation"
        | "comment.error"
        | "comment.note"
        | "comment.todo"
        | "comment.warning" => SynTok::Comment,
        "string" | "string.escape" | "string.regexp" | "string.special" | "symbol"
        | "character" | "character.special" | "escape" => SynTok::String,
        "number" | "boolean" | "constant.builtin" => SynTok::Number,
        "keyword"
        | "keyword.conditional"
        | "keyword.conditional.ternary"
        | "keyword.coroutine"
        | "keyword.directive"
        | "keyword.exception"
        | "keyword.export"
        | "keyword.function"
        | "keyword.import"
        | "keyword.modifier"
        | "keyword.operator"
        | "keyword.repeat"
        | "keyword.return" => SynTok::Keyword,
        "keyword.type" | "type" | "type.builtin" | "type.definition" | "constructor"
        | "tag" | "module" | "module.builtin" | "namespace" | "annotation" => {
            SynTok::Type
        }
        "function"
        | "function.builtin"
        | "function.call"
        | "function.method"
        | "function.method.call" => SynTok::Function,
        "punctuation"
        | "punctuation.bracket"
        | "punctuation.delimiter"
        | "punctuation.special"
        | "operator"
        | "tag.delimiter" => SynTok::Punct,
        "constant" | "variable.builtin" | "variable.super" => SynTok::Type,
        "property" | "field" | "variable.member" | "tag.attribute" | "attribute" => {
            SynTok::Function
        }
        _ => SynTok::Plain,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push_range(
    out: &mut Vec<(SynTok, usize, usize)>,
    kind: SynTok,
    start: usize,
    end: usize,
) {
    if start < end {
        out.push((kind, start, end));
    }
}

/// Map a token kind to a theme-derived color. The `syn_*` palette on
/// `IdeTheme` mirrors `nvim_runtime/lua/rio/theme.lua` per theme so the
/// preview reads with the SAME colors the editor's treesitter / LSP
/// highlighter paints — no theme-mismatch between the finder and the
/// editor.
pub fn syn_color(tok: SynTok, theme: &IdeTheme, dim: bool) -> [u8; 4] {
    let alpha = if dim { 220 } else { 255 };
    let mut c = match tok {
        SynTok::Plain => theme.u8(theme.fg),
        SynTok::Keyword => theme.u8(theme.syn_keyword),
        SynTok::Type => theme.u8(theme.syn_type),
        SynTok::String => theme.u8(theme.syn_string),
        SynTok::Number => theme.u8(theme.syn_number),
        SynTok::Comment => theme.u8(theme.syn_comment),
        SynTok::Function => theme.u8(theme.syn_func),
        SynTok::Punct => theme.u8(theme.muted),
    };
    c[3] = alpha;
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_token(spans: &[(SynTok, &str)], kind: SynTok, text: &str) -> bool {
        spans
            .iter()
            .any(|(span_kind, span_text)| *span_kind == kind && *span_text == text)
    }

    #[test]
    fn typescript_uses_parser_backed_highlight_categories() {
        let spans = highlight_line(
            "const value = client.fetch<User>(url, true)",
            Lang::Typescript,
        );

        assert!(has_token(&spans, SynTok::Keyword, "const"));
        assert!(has_token(&spans, SynTok::Function, "fetch"));
        assert!(has_token(&spans, SynTok::Type, "User"));
        assert!(has_token(&spans, SynTok::Number, "true"));
        assert!(has_token(&spans, SynTok::Punct, "("));
    }

    #[test]
    fn rust_uses_parser_backed_operator_and_escape_highlighting() {
        let spans = highlight_line("let path = format!(\"a\\nb\");", Lang::Rust);

        assert!(has_token(&spans, SynTok::Keyword, "let"));
        assert!(has_token(&spans, SynTok::Function, "format"));
        assert!(has_token(&spans, SynTok::String, "\\n"));
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == SynTok::Punct && *text == "("));
    }
}
