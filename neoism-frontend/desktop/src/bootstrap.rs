//! First-run bootstrap: make a bare tarball install self-sufficient.
//!
//! On every launch a background thread checks for and installs anything a
//! `./install.sh` run would have set up that a plain binary drop is missing:
//!   - bundled Tree-sitter parsers + highlight queries (shipped as a
//!     `runtime/` dir next to the binary by the release tarball; see
//!     `scripts/build-treesitter-runtime.sh`)
//!   - the `xterm-rio`/`rio` terminfo entry (compiled into `~/.terminfo`)
//!   - the Linux desktop launcher + icons
//!
//! Everything is idempotent (cheap stat checks on repeat launches) and
//! best-effort: failures are logged and never block or fail the launch.

use std::fs;
use std::path::{Path, PathBuf};

const TERMINFO_SOURCE: &str = include_str!("../../../misc/rio.terminfo");
#[cfg(target_os = "linux")]
const DESKTOP_ENTRY: &str = include_str!("../../../misc/neoism.desktop");
#[cfg(target_os = "linux")]
const ICON_PNG: &[u8] = include_bytes!("../assets/icons/neoism.png");
#[cfg(target_os = "linux")]
const ICON_SVG: &str = include_str!("../assets/splash/neoism-wordmark.svg");

pub fn spawn() {
    std::thread::Builder::new()
        .name("neoism-bootstrap".into())
        .spawn(|| {
            install_bundled_treesitter_runtime();
            #[cfg(unix)]
            install_terminfo();
            #[cfg(target_os = "linux")]
            install_desktop_entry();
        })
        .ok();
}

//  Bundled Tree-sitter runtime

fn bundled_runtime_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    [dir.join("runtime"), dir.join("../share/neoism/runtime")]
        .into_iter()
        .find(|candidate| candidate.join("RUNTIME_VERSION").is_file())
}

fn install_bundled_treesitter_runtime() {
    let Some(bundle) = bundled_runtime_dir() else {
        return;
    };
    let version = match fs::read_to_string(bundle.join("RUNTIME_VERSION")) {
        Ok(version) => version.trim().to_string(),
        Err(_) => return,
    };

    let dest = neoism_backend::performer::nvim::rio_nvim_runtime_dir();
    let marker = dest.join(".bundled-runtime-version");
    if fs::read_to_string(&marker)
        .map(|installed| installed.trim() == version)
        .unwrap_or(false)
    {
        return;
    }

    let mut copied = 0usize;
    for sub in ["parser", "queries"] {
        let src = bundle.join(sub);
        if !src.is_dir() {
            continue;
        }
        if let Err(err) = copy_tree(&src, &dest.join(sub), &mut copied) {
            tracing::warn!(%err, sub, "bootstrap: bundled tree-sitter copy failed");
            return; // no marker on failure — retry next launch
        }
    }

    if let Err(err) = fs::write(&marker, format!("{version}\n")) {
        tracing::warn!(%err, "bootstrap: failed writing runtime version marker");
        return;
    }
    tracing::info!(
        files = copied,
        version = %version,
        "bootstrap: installed bundled tree-sitter runtime"
    );
}

pub(crate) fn copy_tree(
    src: &Path,
    dest: &Path,
    copied: &mut usize,
) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let to = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &to, copied)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &to)?;
            *copied += 1;
        }
    }
    Ok(())
}

//  Terminfo

#[cfg(unix)]
fn terminfo_installed() -> bool {
    let mut candidates = vec![
        PathBuf::from("/usr/share/terminfo/x/xterm-rio"),
        PathBuf::from("/usr/lib/terminfo/x/xterm-rio"),
        PathBuf::from("/etc/terminfo/x/xterm-rio"),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".terminfo/x/xterm-rio"));
        // Darwin ncurses hashes the leading char as hex ('x' == 0x78).
        candidates.push(home.join(".terminfo/78/xterm-rio"));
    }
    candidates.iter().any(|path| path.is_file())
}

#[cfg(unix)]
fn install_terminfo() {
    if terminfo_installed() {
        return;
    }
    let Some(home) = dirs::home_dir() else {
        return;
    };

    let source_path = std::env::temp_dir().join("neoism-rio.terminfo");
    if fs::write(&source_path, TERMINFO_SOURCE).is_err() {
        return;
    }
    let status = std::process::Command::new("tic")
        .arg("-xe")
        .arg("xterm-rio,rio")
        .arg("-o")
        .arg(home.join(".terminfo"))
        .arg(&source_path)
        .status();
    let _ = fs::remove_file(&source_path);
    match status {
        Ok(status) if status.success() => {
            tracing::info!("bootstrap: installed rio terminfo into ~/.terminfo");
        }
        Ok(status) => {
            tracing::warn!(%status, "bootstrap: tic failed to compile terminfo");
        }
        Err(err) => {
            tracing::warn!(%err, "bootstrap: tic unavailable; skipped terminfo install");
        }
    }
}

//  Desktop launcher + icons (Linux)

#[cfg(target_os = "linux")]
fn install_desktop_entry() {
    // Sandboxed installs manage their own launchers.
    if Path::new("/.flatpak-info").exists() {
        return;
    }
    let Some(data) = dirs::data_local_dir() else {
        return;
    };

    let desktop_path = data.join("applications/neoism.desktop");
    let icons = data.join("icons/hicolor");
    let png_path = icons.join("512x512/apps/neoism.png");
    let svg_path = icons.join("scalable/apps/neoism.svg");
    let mut wrote = false;

    if !desktop_path.exists() {
        if let Ok(exe) = std::env::current_exe() {
            let exe = exe.display().to_string();
            let contents = DESKTOP_ENTRY
                .replace("TryExec=neoism\n", &format!("TryExec={exe}\n"))
                .replace("Exec=neoism\n", &format!("Exec={exe}\n"))
                .replace(
                    "Exec=neoism --new-window",
                    &format!("Exec={exe} --new-window"),
                );
            if write_if_dir_creatable(&desktop_path, contents.as_bytes()) {
                wrote = true;
            }
        }
    }
    if !png_path.exists() && write_if_dir_creatable(&png_path, ICON_PNG) {
        wrote = true;
    }
    if !svg_path.exists() && write_if_dir_creatable(&svg_path, ICON_SVG.as_bytes()) {
        wrote = true;
    }

    if wrote {
        tracing::info!("bootstrap: installed desktop launcher and icons");
        let _ = std::process::Command::new("update-desktop-database")
            .arg(data.join("applications"))
            .status();
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args(["-f", "-t", "--ignore-theme-index"])
            .arg(&icons)
            .status();
    }
}

#[cfg(target_os = "linux")]
fn write_if_dir_creatable(path: &Path, contents: &[u8]) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    if fs::create_dir_all(parent).is_err() {
        return false;
    }
    match fs::write(path, contents) {
        Ok(()) => true,
        Err(err) => {
            tracing::warn!(%err, path = %path.display(), "bootstrap: write failed");
            false
        }
    }
}
