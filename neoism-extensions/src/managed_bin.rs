//! Bridge from `InstalledIndex` to the nvim runtime's managed tool layer.
//!
//! `lsp.lua` builds each LSP / formatter command from a hardcoded bare
//! bin name (e.g. `rust-analyzer` or `black`) and falls back to `$PATH`
//! resolution. When a user installs a tool through the Extensions UI, we drop a
//! managed binary under `paths::bin_dir()` and record it in
//! `installed.json`. To let the lua side prefer that managed binary
//! over `$PATH`, we serialize a `{ id → absolute_path }` map and push
//! it across the embed RPC on init.
//!
//! Keying by `id` is intentional: the lua side stores the id (or the
//! original `cmd[1]`) and resolves at LSP-attach time, so either form
//! works. Entries without a `bin_path` (server-side packages, scripts
//! installed without a managed binary) are skipped — they have nothing
//! to offer the resolver.
//!
//! Cross-name fallback: lua's `server.name` and Mason's package id can
//! diverge for some LSPs (e.g. lua `jsonls` vs Mason `json-lsp`, lua
//! `ts_ls` vs Mason `typescript-language-server`). Lua's resolver tries
//! `managed_bin[server.name]` first, then `managed_bin[server.cmd[1]]`.
//! To cover the cases where neither matches the install id, we also
//! insert each entry keyed by its bin filename (e.g.
//! `vscode-json-language-server`), which is exactly what lua's
//! `cmd[1]` carries.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::install_runner::InstallError;
use crate::installed::InstalledIndex;
use crate::paths;

/// Build a `{ extension_id → absolute_bin_path }` map from the live
/// `installed.json` index. Entries without a `bin_path` are dropped
/// — they cannot satisfy a `cmd[1]` lookup. Empty result is valid
/// (fresh install, nothing managed yet).
pub fn managed_bin_map() -> Result<BTreeMap<String, String>, InstallError> {
    managed_bin_map_from(&paths::installed_record_path())
}

/// Path-explicit variant for tests. Keeps the production path
/// untouched while letting tests point at a temp `installed.json`.
pub fn managed_bin_map_from(
    path: &Path,
) -> Result<BTreeMap<String, String>, InstallError> {
    let index = InstalledIndex::load_from(path)?;
    let mut out = BTreeMap::new();
    for (id, entry) in index.entries.iter() {
        if let Some(bin) = entry.bin_path.as_ref() {
            let resolved_bin = preferred_lsp_bin_path(id, bin);
            let bin_str = resolved_bin.display().to_string();
            out.insert(id.clone(), bin_str.clone());
            // Also key by bin filename to cover lua server names that
            // differ from the Mason install id (lua resolver tries both
            // server.name and server.cmd[1]; cmd[1] is the bin name).
            if let Some(file_name) = resolved_bin.file_name().and_then(|s| s.to_str()) {
                out.entry(file_name.to_string()).or_insert(bin_str);
            }
        }
    }
    Ok(out)
}

fn preferred_lsp_bin_path(id: &str, bin: &Path) -> PathBuf {
    if id == "pyright" {
        if let Some(path) = sibling_bin(bin, "pyright-langserver") {
            return path;
        }
    }
    bin.to_path_buf()
}

fn sibling_bin(bin: &Path, name: &str) -> Option<PathBuf> {
    if let Some(candidate) = sibling_from_parent(bin, name) {
        return Some(candidate);
    }
    if let Ok(target) = std::fs::read_link(bin) {
        let resolved_target = if target.is_absolute() {
            target
        } else {
            bin.parent()?.join(target)
        };
        if let Some(candidate) = sibling_from_parent(&resolved_target, name) {
            return Some(candidate);
        }
    }
    let canonical = std::fs::canonicalize(bin).ok()?;
    sibling_from_parent(&canonical, name)
}

fn sibling_from_parent(bin: &Path, name: &str) -> Option<PathBuf> {
    let candidate = bin.parent()?.join(name);
    candidate.exists().then_some(candidate)
}

/// JSON-encode `managed_bin_map()` for shipping across the nvim RPC.
/// On any error (missing index, bad JSON, IO) returns `"{}"` so the
/// lua side just sees an empty map and falls back to `$PATH`.
pub fn managed_bin_map_json() -> String {
    managed_bin_map()
        .ok()
        .and_then(|m| serde_json::to_string(&m).ok())
        .unwrap_or_else(|| "{}".to_string())
}

/// Path-explicit JSON variant for tests.
pub fn managed_bin_map_json_from(path: &Path) -> String {
    managed_bin_map_from(path)
        .ok()
        .and_then(|m| serde_json::to_string(&m).ok())
        .unwrap_or_else(|| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installed::{InstalledEntry, InstalledIndex};
    use std::path::PathBuf;

    fn tempdir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "neoism-extensions-managed-bin-{}-{}",
            std::process::id(),
            nanoid()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn nanoid() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{:x}", n)
    }

    fn entry(id: &str, bin: Option<&str>) -> InstalledEntry {
        InstalledEntry {
            id: id.to_string(),
            version: "1.0.0".to_string(),
            install_kind: "test".to_string(),
            bin_path: bin.map(PathBuf::from),
            installed_at: 0,
        }
    }

    #[test]
    fn empty_index_yields_empty_json_object() {
        let dir = tempdir();
        let path = dir.join("installed.json");
        // No file written — `load_from` returns default.
        let json = managed_bin_map_json_from(&path);
        assert_eq!(json, "{}");
    }

    #[test]
    fn map_includes_entries_with_bin_paths() {
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(entry(
            "rust-analyzer",
            Some("/opt/neoism/bin/rust-analyzer"),
        ));
        idx.install_record(entry("pyright", Some("/opt/neoism/bin/pyright-langserver")));
        idx.save_to(&path).unwrap();

        let map = managed_bin_map_from(&path).unwrap();
        // 2 id-keyed entries + 1 bin-filename alias for pyright's
        // `pyright-langserver` (rust-analyzer's bin filename equals its
        // id, so its alias is a no-op).
        assert_eq!(map.len(), 3);
        assert_eq!(
            map.get("rust-analyzer").map(String::as_str),
            Some("/opt/neoism/bin/rust-analyzer")
        );
        assert_eq!(
            map.get("pyright").map(String::as_str),
            Some("/opt/neoism/bin/pyright-langserver")
        );
        assert_eq!(
            map.get("pyright-langserver").map(String::as_str),
            Some("/opt/neoism/bin/pyright-langserver")
        );
    }

    #[test]
    fn entries_without_bin_path_are_skipped() {
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(entry("scripts-only", None));
        idx.install_record(entry("real", Some("/x/y/real")));
        idx.save_to(&path).unwrap();

        let map = managed_bin_map_from(&path).unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("real"));
        assert!(!map.contains_key("scripts-only"));
    }

    #[test]
    fn typescript_language_server_id_resolves_via_cmd1_fallback() {
        // lua's `ts_ls` server uses cmd[1] = "typescript-language-server",
        // which matches the Mason install id. The id-keyed lookup is the
        // hit path here; the bin-filename alias matches the same key, so
        // the map carries exactly one entry under that key.
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(entry(
            "typescript-language-server",
            Some("/opt/neoism/bin/typescript-language-server"),
        ));
        idx.save_to(&path).unwrap();

        let map = managed_bin_map_from(&path).unwrap();
        // Lua resolves `managed_bin[server.name="ts_ls"]` → miss, then
        // `managed_bin[server.cmd[1]="typescript-language-server"]` → hit.
        assert_eq!(
            map.get("typescript-language-server").map(String::as_str),
            Some("/opt/neoism/bin/typescript-language-server")
        );
    }

    #[test]
    fn json_lsp_id_resolves_via_bin_filename_alias() {
        // Mason install id = "json-lsp"; lua's `jsonls` server uses
        // cmd[1] = "vscode-json-language-server". Neither matches the id
        // directly, so the bin-filename alias is the only path that
        // resolves the managed binary.
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(entry(
            "json-lsp",
            Some("/opt/neoism/bin/vscode-json-language-server"),
        ));
        idx.save_to(&path).unwrap();

        let map = managed_bin_map_from(&path).unwrap();
        assert_eq!(
            map.get("json-lsp").map(String::as_str),
            Some("/opt/neoism/bin/vscode-json-language-server")
        );
        assert_eq!(
            map.get("vscode-json-language-server").map(String::as_str),
            Some("/opt/neoism/bin/vscode-json-language-server")
        );
    }

    #[test]
    fn pyright_prefers_sibling_langserver_for_existing_installs() {
        let dir = tempdir();
        let npm_bin = dir.join("node_modules").join(".bin");
        std::fs::create_dir_all(&npm_bin).unwrap();
        let pyright = npm_bin.join("pyright");
        let pyright_langserver = npm_bin.join("pyright-langserver");
        std::fs::write(&pyright, b"#!/usr/bin/env node\n").unwrap();
        std::fs::write(&pyright_langserver, b"#!/usr/bin/env node\n").unwrap();

        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(entry("pyright", Some(pyright.to_str().unwrap())));
        idx.save_to(&path).unwrap();

        let map = managed_bin_map_from(&path).unwrap();
        let expected = pyright_langserver.display().to_string();
        assert_eq!(map.get("pyright"), Some(&expected));
        assert_eq!(map.get("pyright-langserver"), Some(&expected));
    }

    #[cfg(unix)]
    #[test]
    fn pyright_prefers_sibling_langserver_for_symlinked_wrapper() {
        let dir = tempdir();
        let public_bin = dir.join("extensions").join("bin");
        let npm_bin = dir
            .join("extensions")
            .join("installed")
            .join("pyright")
            .join("node_modules")
            .join(".bin");
        std::fs::create_dir_all(&public_bin).unwrap();
        std::fs::create_dir_all(&npm_bin).unwrap();
        let pyright = npm_bin.join("pyright");
        let pyright_langserver = npm_bin.join("pyright-langserver");
        std::fs::write(&pyright, b"#!/usr/bin/env node\n").unwrap();
        std::fs::write(&pyright_langserver, b"#!/usr/bin/env node\n").unwrap();
        let wrapped_pyright = public_bin.join("pyright");
        std::os::unix::fs::symlink(&pyright, &wrapped_pyright).unwrap();

        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(entry("pyright", Some(wrapped_pyright.to_str().unwrap())));
        idx.save_to(&path).unwrap();

        let map = managed_bin_map_from(&path).unwrap();
        let expected = pyright_langserver.display().to_string();
        assert_eq!(map.get("pyright"), Some(&expected));
        assert_eq!(map.get("pyright-langserver"), Some(&expected));
    }

    #[test]
    fn bin_filename_alias_does_not_overwrite_existing_id_entry() {
        // Name collision: one entry's `id` equals another entry's bin
        // filename. The id-keyed entry must win — the alias is
        // best-effort and uses `or_insert`, so it never clobbers a real
        // id. The id key is also inserted last via `.insert()`, so
        // even if the alias landed first it will be overwritten by the
        // canonical id-keyed entry on the corresponding iteration.
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        // BTreeMap iter is alphabetic: `aaa-other` is visited before
        // `foo`, so the alias `foo` lands first; the canonical `foo`
        // id-keyed entry then overwrites it.
        idx.install_record(entry("aaa-other", Some("/alt/foo")));
        idx.install_record(entry("foo", Some("/canonical/foo")));
        idx.save_to(&path).unwrap();

        let map = managed_bin_map_from(&path).unwrap();
        assert_eq!(map.get("foo").map(String::as_str), Some("/canonical/foo"));
        assert_eq!(map.get("aaa-other").map(String::as_str), Some("/alt/foo"));
    }

    #[test]
    fn json_round_trips_into_a_map() {
        let dir = tempdir();
        let path = dir.join("installed.json");
        let mut idx = InstalledIndex::default();
        idx.install_record(entry("a", Some("/bin/a")));
        idx.install_record(entry("b", Some("/bin/b")));
        idx.save_to(&path).unwrap();

        let json = managed_bin_map_json_from(&path);
        let decoded: BTreeMap<String, String> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.get("a").map(String::as_str), Some("/bin/a"));
        assert_eq!(decoded.get("b").map(String::as_str), Some("/bin/b"));
    }
}
