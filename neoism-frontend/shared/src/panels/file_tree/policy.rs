use std::path::{Component, Path, PathBuf};

use super::types::{NodeKind, TreeEntry, VirtualEntryKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileTreeBridgeState {
    pub visible: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileTreeVisibilityDecision {
    pub visible: bool,
    pub focused: bool,
    pub visibility_changed: bool,
    pub refresh_workspace_root: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryLinkDecision {
    pub reveal_root: PathBuf,
    pub visible: bool,
    pub focused: bool,
    pub visibility_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionActivation {
    None,
    OpenPath(PathBuf),
    OpenVirtual(VirtualEntryKind),
    ToggleDirectory { index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameTarget {
    Noop,
    Target(PathBuf),
}

pub fn toggle_visibility_policy(
    state: FileTreeBridgeState,
) -> FileTreeVisibilityDecision {
    if !state.visible {
        FileTreeVisibilityDecision {
            visible: true,
            focused: true,
            visibility_changed: true,
            refresh_workspace_root: true,
        }
    } else if state.focused {
        FileTreeVisibilityDecision {
            visible: false,
            focused: false,
            visibility_changed: true,
            refresh_workspace_root: false,
        }
    } else {
        FileTreeVisibilityDecision {
            visible: true,
            focused: true,
            visibility_changed: false,
            refresh_workspace_root: false,
        }
    }
}

pub fn open_command_policy(state: FileTreeBridgeState) -> FileTreeVisibilityDecision {
    FileTreeVisibilityDecision {
        visible: true,
        focused: true,
        visibility_changed: !state.visible,
        refresh_workspace_root: !state.visible,
    }
}

pub fn close_policy(state: FileTreeBridgeState) -> Option<FileTreeVisibilityDecision> {
    state.visible.then_some(FileTreeVisibilityDecision {
        visible: false,
        focused: false,
        visibility_changed: true,
        refresh_workspace_root: false,
    })
}

pub fn directory_link_policy(
    dir: &Path,
    current_root: Option<&Path>,
    active_workspace_root: Option<&Path>,
    active_pane_root: Option<&Path>,
    was_visible: bool,
) -> DirectoryLinkDecision {
    let reveal_root = current_root
        .into_iter()
        .chain(active_workspace_root)
        .chain(active_pane_root)
        .find(|root| dir != *root && dir.starts_with(root))
        .map(Path::to_path_buf)
        .or_else(|| dir.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| dir.to_path_buf());

    DirectoryLinkDecision {
        reveal_root,
        visible: true,
        focused: true,
        visibility_changed: !was_visible,
    }
}

pub fn activation_for_selection(
    selected: Option<&TreeEntry>,
    selected_index: usize,
) -> SelectionActivation {
    let Some(selected) = selected else {
        return SelectionActivation::None;
    };
    if let Some(kind) = selected.virtual_kind {
        if kind != VirtualEntryKind::NeoismWorkspace {
            return SelectionActivation::OpenVirtual(kind);
        }
    }

    match selected.kind {
        NodeKind::File => selected
            .path
            .clone()
            .map(SelectionActivation::OpenPath)
            .unwrap_or(SelectionActivation::None),
        NodeKind::Dir { .. } => SelectionActivation::ToggleDirectory {
            index: selected_index,
        },
    }
}

pub fn selected_path_for_entry(selected: Option<&TreeEntry>) -> Option<PathBuf> {
    selected
        .filter(|entry| !entry.is_virtual())
        .and_then(|entry| entry.path.clone())
}

pub fn target_dir_for_selection(
    selected: Option<&TreeEntry>,
    root: Option<&Path>,
    neoism_note_root: Option<&Path>,
) -> Option<PathBuf> {
    if selected.is_some_and(|entry| {
        entry.is_virtual() && !entry.is_neoism_workspace_virtual_root()
    }) {
        return None;
    }

    let selected_dir = selected.and_then(|entry| {
        if entry.is_neoism_workspace_virtual_root() {
            return neoism_note_root.map(Path::to_path_buf);
        }
        if entry.is_virtual() {
            return None;
        }
        let path = entry.path.as_ref()?;
        match entry.kind {
            NodeKind::Dir { .. } => Some(path.clone()),
            NodeKind::File => path.parent().map(Path::to_path_buf),
        }
    });

    selected_dir.or_else(|| root.map(Path::to_path_buf))
}

pub fn rename_target_for_input(path: &Path, name: &str) -> Result<RenameTarget, String> {
    let Some(parent) = path.parent() else {
        return Err("Cannot rename this path.".to_string());
    };
    let target = child_path_for_input(parent, name)?;
    if target == path {
        Ok(RenameTarget::Noop)
    } else {
        Ok(RenameTarget::Target(target))
    }
}

fn child_path_for_input(base_dir: &Path, input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty.".to_string());
    }
    let rel = Path::new(trimmed);
    if rel.is_absolute() {
        return Err("Use a relative name.".to_string());
    }
    if rel.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("Name cannot climb out of the selected folder.".to_string());
    }
    Ok(base_dir.join(rel))
}

// ---------------------------------------------------------------------------
// populate / refresh-driven git-status worker decision
// ---------------------------------------------------------------------------

/// Inputs the host has gathered about the file-tree git refresh
/// state machine before deciding whether to kick a worker, queue a
/// pending request, or short-circuit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileTreeGitRefreshState {
    /// Tree is currently visible — hidden trees skip the worker
    /// entirely.
    pub visible: bool,
    /// A worker spawned by a previous tick has not reported back yet.
    pub inflight: bool,
    /// Self-event suppression window is active (a populate/refresh
    /// fired recently, so the daemon's matching file-system event
    /// would re-trigger us in a loop).
    pub self_event_suppressed: bool,
}

/// What the host should do for a refresh request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeGitRefreshAction {
    /// Tree hidden / suppressed — drop the request and clear any
    /// queued pending flag.
    Drop,
    /// A worker is already running — flip `git_refresh_pending` so
    /// the next completion re-arms a fresh request.
    Queue,
    /// Kick a fresh worker now.
    Spawn,
}

/// Decide whether a `refresh_file_tree_git_status` / `populate`
/// kickoff should spawn a worker, queue, or short-circuit. Mirrors
/// the gating that lives in the desktop bridge's
/// `start_file_tree_git_status_refresh` + `refresh_file_tree_git_status`
/// — pulled out so the same rules can be exercised by the web host.
pub fn file_tree_git_refresh_action(
    state: FileTreeGitRefreshState,
) -> FileTreeGitRefreshAction {
    if !state.visible {
        return FileTreeGitRefreshAction::Drop;
    }
    if state.self_event_suppressed {
        return FileTreeGitRefreshAction::Drop;
    }
    if state.inflight {
        return FileTreeGitRefreshAction::Queue;
    }
    FileTreeGitRefreshAction::Spawn
}

// ---------------------------------------------------------------------------
// Context-menu item construction
// ---------------------------------------------------------------------------

/// What the host has resolved about the currently-selected file-tree
/// row when building the context menu / actions modal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileTreeContextMenuInputs {
    /// `Some(path)` only for non-virtual rows that actually carry a
    /// filesystem path. Virtual placeholders pass `None` here.
    pub path: Option<PathBuf>,
    /// Destination directory for paste / new-file / new-folder
    /// actions. `None` suppresses the directory-oriented buttons.
    pub target_dir: Option<PathBuf>,
}

/// Plan describing which item rows the context menu should contain
/// (in order). The host turns each entry into the concrete
/// `ContextMenuItem` / `ModalButton` with the matching action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeContextItem {
    EditOrOpen,
    Copy,
    Paste,
    NewFile,
    NewFolder,
    Rename,
    Delete,
}

impl FileTreeContextItem {
    /// Display label exactly matching the desktop bridge.
    pub fn label(self) -> &'static str {
        match self {
            FileTreeContextItem::EditOrOpen => "Edit / Open",
            FileTreeContextItem::Copy => "Copy",
            FileTreeContextItem::Paste => "Paste Here",
            FileTreeContextItem::NewFile => "New File",
            FileTreeContextItem::NewFolder => "New Folder",
            FileTreeContextItem::Rename => "Rename",
            FileTreeContextItem::Delete => "Delete",
        }
    }

    /// Mnemonic/hint shown next to the label in the context menu.
    pub fn hint(self) -> &'static str {
        match self {
            FileTreeContextItem::EditOrOpen => "e",
            FileTreeContextItem::Copy => "c",
            FileTreeContextItem::Paste => "p",
            FileTreeContextItem::NewFile => "n",
            FileTreeContextItem::NewFolder => "f",
            FileTreeContextItem::Rename => "r",
            FileTreeContextItem::Delete => "d",
        }
    }
}

/// Build the ordered list of items the file-tree context menu /
/// actions modal should show, given which rails the selection
/// exposes. The desktop bridge keeps owning the actual
/// `ContextMenuItem` / `ModalButton` construction (those types carry
/// host-only `ContextMenuAction` payloads), but the *which buttons
/// and in what order* decision now lives here so the web host shares
/// it verbatim.
pub fn file_tree_context_menu_items(
    inputs: &FileTreeContextMenuInputs,
) -> Vec<FileTreeContextItem> {
    let mut items = Vec::new();
    if inputs.path.is_some() {
        items.push(FileTreeContextItem::EditOrOpen);
        items.push(FileTreeContextItem::Copy);
    }
    if inputs.target_dir.is_some() {
        items.push(FileTreeContextItem::Paste);
        items.push(FileTreeContextItem::NewFile);
        items.push(FileTreeContextItem::NewFolder);
    }
    if inputs.path.is_some() {
        items.push(FileTreeContextItem::Rename);
        items.push(FileTreeContextItem::Delete);
    }
    items
}

/// Returns `true` if the host should suppress opening the context
/// menu / actions modal entirely: there's neither a path-bearing
/// selection nor a target directory to write into.
pub fn file_tree_context_menu_should_open(inputs: &FileTreeContextMenuInputs) -> bool {
    inputs.path.is_some() || inputs.target_dir.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_refresh_drops_when_hidden_or_suppressed() {
        assert_eq!(
            file_tree_git_refresh_action(FileTreeGitRefreshState {
                visible: false,
                inflight: false,
                self_event_suppressed: false,
            }),
            FileTreeGitRefreshAction::Drop
        );
        assert_eq!(
            file_tree_git_refresh_action(FileTreeGitRefreshState {
                visible: true,
                inflight: false,
                self_event_suppressed: true,
            }),
            FileTreeGitRefreshAction::Drop
        );
    }

    #[test]
    fn git_refresh_queues_while_worker_inflight() {
        assert_eq!(
            file_tree_git_refresh_action(FileTreeGitRefreshState {
                visible: true,
                inflight: true,
                self_event_suppressed: false,
            }),
            FileTreeGitRefreshAction::Queue
        );
    }

    #[test]
    fn git_refresh_spawns_when_idle_and_visible() {
        assert_eq!(
            file_tree_git_refresh_action(FileTreeGitRefreshState {
                visible: true,
                inflight: false,
                self_event_suppressed: false,
            }),
            FileTreeGitRefreshAction::Spawn
        );
    }

    #[test]
    fn context_menu_empty_when_nothing_selected() {
        let items = file_tree_context_menu_items(&FileTreeContextMenuInputs::default());
        assert!(items.is_empty());
        assert!(!file_tree_context_menu_should_open(
            &FileTreeContextMenuInputs::default()
        ));
    }

    #[test]
    fn context_menu_includes_path_actions_when_selected() {
        let inputs = FileTreeContextMenuInputs {
            path: Some(PathBuf::from("/tmp/x")),
            target_dir: None,
        };
        let items = file_tree_context_menu_items(&inputs);
        assert_eq!(
            items,
            vec![
                FileTreeContextItem::EditOrOpen,
                FileTreeContextItem::Copy,
                FileTreeContextItem::Rename,
                FileTreeContextItem::Delete,
            ]
        );
        assert!(file_tree_context_menu_should_open(&inputs));
    }

    #[test]
    fn context_menu_dir_only_shows_paste_and_new_actions() {
        let inputs = FileTreeContextMenuInputs {
            path: None,
            target_dir: Some(PathBuf::from("/tmp")),
        };
        let items = file_tree_context_menu_items(&inputs);
        assert_eq!(
            items,
            vec![
                FileTreeContextItem::Paste,
                FileTreeContextItem::NewFile,
                FileTreeContextItem::NewFolder,
            ]
        );
    }

    #[test]
    fn context_menu_combines_path_and_dir_actions_in_order() {
        let inputs = FileTreeContextMenuInputs {
            path: Some(PathBuf::from("/tmp/x")),
            target_dir: Some(PathBuf::from("/tmp")),
        };
        let items = file_tree_context_menu_items(&inputs);
        assert_eq!(
            items,
            vec![
                FileTreeContextItem::EditOrOpen,
                FileTreeContextItem::Copy,
                FileTreeContextItem::Paste,
                FileTreeContextItem::NewFile,
                FileTreeContextItem::NewFolder,
                FileTreeContextItem::Rename,
                FileTreeContextItem::Delete,
            ]
        );
    }

    #[test]
    fn context_item_labels_and_hints_match_desktop_strings() {
        assert_eq!(FileTreeContextItem::EditOrOpen.label(), "Edit / Open");
        assert_eq!(FileTreeContextItem::Paste.label(), "Paste Here");
        assert_eq!(FileTreeContextItem::Delete.hint(), "d");
    }
}
