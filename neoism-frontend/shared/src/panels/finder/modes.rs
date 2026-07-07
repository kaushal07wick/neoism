// File / grep search-mode enums (fuzzy / exact / regex).

use crate::services::{SearchFileMode, SearchGrepMode};

/// What the finder is searching over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinderMode {
    /// `rg --files`-collected paths, fuzzy-filtered in-memory.
    Files,
    /// `rg <query>` — re-run on each query change (debounced).
    Grep,
    /// Git porcelain changed files for the current repository.
    #[allow(dead_code)]
    GitChanges,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum FileSearchMode {
    #[default]
    Fuzzy,
    Exact,
}

impl FileSearchMode {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Fuzzy => Self::Exact,
            Self::Exact => Self::Fuzzy,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Fuzzy => "fuzzy",
            Self::Exact => "exact",
        }
    }

    pub(super) fn as_service_mode(self) -> SearchFileMode {
        match self {
            Self::Fuzzy => SearchFileMode::Fuzzy,
            Self::Exact => SearchFileMode::Exact,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum GrepSearchMode {
    #[default]
    Fuzzy,
    Exact,
    Regex,
}

impl GrepSearchMode {
    pub(super) fn next(self) -> Self {
        match self {
            Self::Fuzzy => Self::Exact,
            Self::Exact => Self::Regex,
            Self::Regex => Self::Fuzzy,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Fuzzy => "fuzzy",
            Self::Exact => "exact",
            Self::Regex => "regex",
        }
    }

    pub(super) fn as_service_mode(self) -> SearchGrepMode {
        match self {
            Self::Fuzzy => SearchGrepMode::Fuzzy,
            Self::Exact => SearchGrepMode::Exact,
            Self::Regex => SearchGrepMode::Regex,
        }
    }
}
