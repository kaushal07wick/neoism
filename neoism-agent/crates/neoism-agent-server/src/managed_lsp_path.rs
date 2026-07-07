use std::path::PathBuf;

pub(crate) fn managed_lsp_path_entries() -> Vec<PathBuf> {
    let mut entries = Vec::new();
    if let Some(home) = home_dir() {
        entries.push(
            home.join(".local")
                .join("share")
                .join("neoism")
                .join("extensions")
                .join("bin"),
        );
        let root = home.join(".local").join("share").join("rio").join("lsp");
        entries.push(root.join("bin"));
        entries.push(root.join("node").join("bin"));
        entries.push(root.join("nix-profile").join("bin"));
        entries.push(home.join(".cargo").join("bin"));
    }
    entries
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub(crate) fn managed_lsp_path() -> Option<std::ffi::OsString> {
    let mut paths = managed_lsp_path_entries();
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).ok()
}
