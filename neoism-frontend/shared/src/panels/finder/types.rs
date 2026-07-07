// Result row types + git change classification.

use crate::primitives::IdeTheme;
use crate::services::SearchGitStatus;

#[derive(Clone)]
pub(super) struct FileResult {
    pub(super) path: String,
}

#[derive(Clone)]
pub(super) struct GrepResult {
    pub(super) path: String,
    pub(super) line: u32,
    #[allow(dead_code)]
    pub(super) column: u32,
    pub(super) text: String,
}

#[derive(Clone)]
pub(super) struct GitResult {
    pub(super) path: String,
    pub(super) status: GitChangeStatus,
    pub(super) line: u32,
    pub(super) text: String,
}

#[derive(Clone)]
pub(super) enum Result_ {
    File(FileResult),
    Grep(GrepResult),
    Git(GitResult),
}

impl Result_ {
    pub(super) fn path(&self) -> &str {
        match self {
            Result_::File(f) => &f.path,
            Result_::Grep(g) => &g.path,
            Result_::Git(g) => &g.path,
        }
    }

    pub(super) fn line(&self) -> Option<u32> {
        match self {
            Result_::File(_) => None,
            Result_::Grep(g) => Some(g.line),
            Result_::Git(g) => Some(g.line),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GitChangeStatus {
    Modified,
    Staged,
    Mixed,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflict,
}

impl GitChangeStatus {
    pub(super) fn from_service(status: SearchGitStatus) -> Self {
        match status {
            SearchGitStatus::Modified => GitChangeStatus::Modified,
            SearchGitStatus::Staged => GitChangeStatus::Staged,
            SearchGitStatus::Mixed => GitChangeStatus::Mixed,
            SearchGitStatus::Added => GitChangeStatus::Added,
            SearchGitStatus::Deleted => GitChangeStatus::Deleted,
            SearchGitStatus::Renamed => GitChangeStatus::Renamed,
            SearchGitStatus::Untracked => GitChangeStatus::Untracked,
            SearchGitStatus::Conflict => GitChangeStatus::Conflict,
        }
    }

    fn theme_color(self, theme: &IdeTheme) -> u32 {
        match self {
            GitChangeStatus::Modified => theme.yellow,
            GitChangeStatus::Staged => theme.green,
            GitChangeStatus::Mixed => theme.magenta,
            GitChangeStatus::Added => theme.green,
            GitChangeStatus::Deleted => theme.red,
            GitChangeStatus::Renamed => theme.blue,
            GitChangeStatus::Untracked => theme.cyan,
            GitChangeStatus::Conflict => theme.red,
        }
    }

    pub(super) fn marker(self) -> &'static str {
        match self {
            GitChangeStatus::Modified => "M",
            GitChangeStatus::Staged => "S",
            GitChangeStatus::Mixed => "M*",
            GitChangeStatus::Added => "A",
            GitChangeStatus::Deleted => "D",
            GitChangeStatus::Renamed => "R",
            GitChangeStatus::Untracked => "?",
            GitChangeStatus::Conflict => "!",
        }
    }

    pub(super) fn color(self, theme: &IdeTheme) -> [u8; 4] {
        theme.u8(self.theme_color(theme))
    }

    pub(super) fn f32_alpha(self, theme: &IdeTheme, alpha: f32) -> [f32; 4] {
        theme.f32_alpha(self.theme_color(theme), alpha)
    }
}
