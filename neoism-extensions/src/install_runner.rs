use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::installed::InstalledEntry;
use crate::manifest::{ExtensionManifest, GithubAsset, InstallKind};
use crate::paths;

#[derive(Clone, Debug)]
pub enum ProgressEvent {
    Started,
    Downloading { bytes: u64, total: Option<u64> },
    Extracting,
    Linking,
    // Carries both a coarse percent and a status line so the panel can render
    // a single progress bar + label without needing to translate variants.
    Progress { percent: u8, status: String },
    Done,
    Failed { message: String },
}

pub struct InstallHandle {
    pub id: String,
}

impl InstallHandle {
    pub fn cancel(&self) {}
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("network error: {0}")]
    Network(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("command `{command}` failed with status {status}: {stderr}")]
    CommandFailed {
        command: String,
        status: i32,
        stderr: String,
    },
    #[error("install process exited with status {exit:?}; tail: {tail}")]
    Process {
        exit: std::process::ExitStatus,
        tail: String,
    },
    #[error("tool `{0}` not found on PATH")]
    MissingTool(&'static str),
    #[error("no asset matches target `{0}`")]
    NoAssetForTarget(String),
    #[error("binary `{0}` not found after install (archive layout mismatch or failed download)")]
    BinaryNotFound(String),
    #[error("zip error: {0}")]
    Zip(String),
    #[error("extension is already installed")]
    AlreadyInstalled,
    #[error("extension is not installed")]
    NotInstalled,
    #[error("not yet implemented")]
    NotImplemented,
    #[error("config parse error: {0}")]
    ParseManifest(String),
}

/// Spawn the install task and hand back its `JoinHandle`. Progress events
/// flow through `progress`; dropping the sender on completion is the EOF
/// signal for the receiver loop. The final `Done` event is emitted *after*
/// the symlink + record creation succeeds.
pub fn install(
    manifest: ExtensionManifest,
    progress: UnboundedSender<ProgressEvent>,
) -> JoinHandle<Result<InstalledEntry, InstallError>> {
    tokio::spawn(async move { run_install(manifest, progress).await })
}

async fn run_install(
    manifest: ExtensionManifest,
    progress: UnboundedSender<ProgressEvent>,
) -> Result<InstalledEntry, InstallError> {
    emit(&progress, ProgressEvent::Started);

    let install_dir = paths::install_dir_for(&manifest.id);
    tokio::fs::create_dir_all(&install_dir).await?;

    let bin_names: Vec<String> = manifest
        .run
        .as_ref()
        .and_then(|r| r.command.first().cloned())
        .map(|c| vec![c])
        .unwrap_or_default();

    let bin_path = match &manifest.install {
        InstallKind::Npm { package, version } => {
            install_npm(&install_dir, package, version, &bin_names, &progress).await?
        }
        InstallKind::Pip { package, version } => {
            install_pip(&install_dir, package, version, &bin_names, &progress).await?
        }
        InstallKind::GithubRelease {
            owner,
            repo,
            tag,
            assets,
        } => {
            let target = current_target();
            let asset = pick_asset(assets, target)
                .ok_or_else(|| InstallError::NoAssetForTarget(target.to_string()))?;
            install_github_release(&install_dir, owner, repo, tag, asset, &progress)
                .await?
        }
        InstallKind::Cargo { .. } => {
            // Cargo install kind is reserved for a later phase; explicit error
            // beats silently swallowing the install request.
            return Err(InstallError::NotImplemented);
        }
    };

    emit(&progress, ProgressEvent::Linking);
    let link_path = link_bin(&bin_path, default_bin_name(&manifest, &bin_path)).await?;

    let kind = install_kind_tag(&manifest.install);
    let entry = InstalledEntry {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        install_kind: kind.to_string(),
        bin_path: Some(link_path),
        installed_at: now_ms(),
    };

    emit(
        &progress,
        ProgressEvent::Progress {
            percent: 100,
            status: format!("installed {} {}", manifest.id, manifest.version),
        },
    );
    emit(&progress, ProgressEvent::Done);
    Ok(entry)
}

fn install_kind_tag(kind: &InstallKind) -> &'static str {
    match kind {
        InstallKind::Npm { .. } => "npm",
        InstallKind::Pip { .. } => "pip",
        InstallKind::GithubRelease { .. } => "github_release",
        InstallKind::Cargo { .. } => "cargo",
    }
}

fn emit(tx: &UnboundedSender<ProgressEvent>, ev: ProgressEvent) {
    let _ = tx.send(ev);
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// npm
// ---------------------------------------------------------------------------

async fn install_npm(
    install_dir: &Path,
    pkg: &str,
    version: &str,
    bin_names: &[String],
    progress: &UnboundedSender<ProgressEvent>,
) -> Result<PathBuf, InstallError> {
    tokio::fs::create_dir_all(install_dir).await?;

    let pkg_spec = format!("{pkg}@{version}");
    let mut cmd = Command::new("npm");
    cmd.arg("install")
        .arg("--prefix")
        .arg(install_dir)
        .arg("--no-audit")
        .arg("--no-fund")
        .arg("--loglevel=info")
        .arg(&pkg_spec)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(InstallError::MissingTool("npm"))
        }
        Err(e) => return Err(InstallError::Io(e)),
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tail = drive_command_progress(stdout, stderr, progress).await;

    let status = child.wait().await?;
    if !status.success() {
        return Err(InstallError::Process {
            exit: status,
            tail: tail.join("\n"),
        });
    }

    resolve_npm_bin(install_dir, pkg, bin_names).await
}

async fn resolve_npm_bin(
    install_dir: &Path,
    pkg: &str,
    bin_names: &[String],
) -> Result<PathBuf, InstallError> {
    let bin_dir = install_dir.join("node_modules").join(".bin");
    if let Some(first) = bin_names.first() {
        return Ok(npm_bin_path(&bin_dir, first));
    }

    // Fallback: read package.json's `bin` field.
    let pkg_json = install_dir
        .join("node_modules")
        .join(pkg)
        .join("package.json");
    let bytes = tokio::fs::read(&pkg_json).await?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| InstallError::ParseManifest(format!("package.json: {e}")))?;
    let bin_field = value.get("bin").ok_or_else(|| {
        InstallError::ParseManifest(format!(
            "package.json missing `bin`: {}",
            pkg_json.display()
        ))
    })?;
    let first_name: String = match bin_field {
        serde_json::Value::String(_) => pkg.rsplit('/').next().unwrap_or(pkg).to_string(),
        serde_json::Value::Object(map) => {
            map.keys().next().cloned().ok_or_else(|| {
                InstallError::ParseManifest("empty `bin` map".to_string())
            })?
        }
        _ => {
            return Err(InstallError::ParseManifest(
                "`bin` must be string or object".to_string(),
            ))
        }
    };
    Ok(npm_bin_path(&bin_dir, &first_name))
}

fn npm_bin_path(bin_dir: &Path, name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        bin_dir.join(format!("{name}.cmd"))
    }
    #[cfg(not(windows))]
    {
        bin_dir.join(name)
    }
}

// ---------------------------------------------------------------------------
// pip
// ---------------------------------------------------------------------------

async fn install_pip(
    install_dir: &Path,
    pkg: &str,
    version: &str,
    bin_names: &[String],
    progress: &UnboundedSender<ProgressEvent>,
) -> Result<PathBuf, InstallError> {
    tokio::fs::create_dir_all(install_dir).await?;

    let venv_dir = install_dir.join("venv");
    let mut venv_cmd = Command::new("python3");
    venv_cmd
        .arg("-m")
        .arg("venv")
        .arg(&venv_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut venv_child = match venv_cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(InstallError::MissingTool("python3"))
        }
        Err(e) => return Err(InstallError::Io(e)),
    };
    let venv_stdout = venv_child.stdout.take();
    let venv_stderr = venv_child.stderr.take();
    let venv_tail = drive_command_progress(venv_stdout, venv_stderr, progress).await;
    let venv_status = venv_child.wait().await?;
    if !venv_status.success() {
        return Err(InstallError::Process {
            exit: venv_status,
            tail: venv_tail.join("\n"),
        });
    }

    let pip_bin = venv_pip_path(&venv_dir);
    let pkg_spec = format!("{pkg}=={version}");
    let mut pip_cmd = Command::new(&pip_bin);
    pip_cmd
        .arg("install")
        .arg(&pkg_spec)
        .arg("--no-input")
        .arg("--progress-bar")
        .arg("off")
        .arg("--disable-pip-version-check")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut pip_child = pip_cmd.spawn()?;
    let pip_stdout = pip_child.stdout.take();
    let pip_stderr = pip_child.stderr.take();
    let pip_tail = drive_command_progress(pip_stdout, pip_stderr, progress).await;
    let pip_status = pip_child.wait().await?;
    if !pip_status.success() {
        return Err(InstallError::Process {
            exit: pip_status,
            tail: pip_tail.join("\n"),
        });
    }

    let bin_name = bin_names
        .first()
        .cloned()
        .unwrap_or_else(|| pkg.to_string());
    Ok(venv_bin_path(&venv_dir, &bin_name))
}

fn venv_pip_path(venv: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        venv.join("Scripts").join("pip.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        venv.join("bin").join("pip")
    }
}

fn venv_bin_path(venv: &Path, name: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        venv.join("Scripts").join(format!("{name}.exe"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        venv.join("bin").join(name)
    }
}

// ---------------------------------------------------------------------------
// shared subprocess progress driver
// ---------------------------------------------------------------------------

/// Read stdout + stderr line-by-line, emit progress events, return the last
/// ~20 stderr lines for error reporting.
async fn drive_command_progress(
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    progress: &UnboundedSender<ProgressEvent>,
) -> Vec<String> {
    use std::sync::{Arc, Mutex};

    let tail: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));

    let stdout_task = stdout.map(|s| {
        let progress = progress.clone();
        let count = count.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(s).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let percent = forward_progress(&count, &line);
                let _ = progress.send(ProgressEvent::Progress {
                    percent,
                    status: line,
                });
            }
        })
    });

    let stderr_task = stderr.map(|s| {
        let progress = progress.clone();
        let count = count.clone();
        let tail = tail.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(s).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                {
                    let mut t = tail.lock().unwrap();
                    t.push(line.clone());
                    if t.len() > 20 {
                        let drop_n = t.len() - 20;
                        t.drain(0..drop_n);
                    }
                }
                let percent = forward_progress(&count, &line);
                let _ = progress.send(ProgressEvent::Progress {
                    percent,
                    status: line,
                });
            }
        })
    });

    if let Some(t) = stdout_task {
        let _ = t.await;
    }
    if let Some(t) = stderr_task {
        let _ = t.await;
    }

    let lock = tail.lock().unwrap();
    lock.clone()
}

/// Pick a percent for a subprocess log line. Pip occasionally exposes a
/// real percent; otherwise we count lines and clamp into 5..=90 so the bar
/// never reaches 100% before the symlink stage.
fn forward_progress(count: &std::sync::Arc<std::sync::Mutex<u32>>, line: &str) -> u8 {
    if let Some(pct) = parse_percent(line) {
        return pct.min(90);
    }
    let mut n = count.lock().unwrap();
    *n = n.saturating_add(1);
    let raw = 5u32 + *n * 2;
    raw.min(90) as u8
}

fn parse_percent(line: &str) -> Option<u8> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'%' {
                let n: u32 = line[start..i].parse().ok()?;
                return Some(n.min(100) as u8);
            }
        } else {
            i += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// github release
// ---------------------------------------------------------------------------

async fn install_github_release(
    install_dir: &Path,
    owner: &str,
    repo: &str,
    tag: &str,
    asset: &GithubAsset,
    progress: &UnboundedSender<ProgressEvent>,
) -> Result<PathBuf, InstallError> {
    tokio::fs::create_dir_all(install_dir).await?;
    let staging = install_dir.join("staging");
    tokio::fs::create_dir_all(&staging).await?;

    let url = github_asset_url(owner, repo, tag, &asset.file);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| InstallError::Network(e.to_string()))?
        .error_for_status()
        .map_err(|e| InstallError::Network(e.to_string()))?;
    let total = resp.content_length();

    let staged_file = staging.join(&asset.file);
    let mut out = tokio::fs::File::create(&staged_file).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit: u8 = 0;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| InstallError::Network(e.to_string()))?;
        out.write_all(&bytes).await?;
        downloaded = downloaded.saturating_add(bytes.len() as u64);
        emit(
            progress,
            ProgressEvent::Downloading {
                bytes: downloaded,
                total,
            },
        );
        if let Some(t) = total {
            // Map download into 5..=80% — leave headroom for extract + link.
            if t > 0 {
                let pct = ((downloaded * 75) / t).min(75) as u8 + 5;
                if pct.saturating_sub(last_emit) >= 2 {
                    last_emit = pct;
                    emit(
                        progress,
                        ProgressEvent::Progress {
                            percent: pct,
                            status: format!(
                                "downloading {} ({downloaded}/{t})",
                                asset.file
                            ),
                        },
                    );
                }
            }
        }
    }
    out.flush().await?;
    drop(out);

    emit(progress, ProgressEvent::Extracting);
    emit(
        progress,
        ProgressEvent::Progress {
            percent: 85,
            status: format!("extracting {}", asset.file),
        },
    );

    let bin_target = install_dir.join(&asset.bin);
    extract_asset(&staged_file, install_dir, &bin_target, &asset.file).await?;

    // Resolve where the binary actually landed. `.gz`/raw assets go straight to
    // `bin_target`, but tar.gz/zip archives often NEST it (e.g.
    // `pkg/bin/server`), so if it's not at the expected path, search the
    // extracted tree for a file named `asset.bin`. Without this the install
    // "succeeds" but leaves a dangling symlink → the pill shows "Binary
    // missing" and LSP silently never works.
    let bin_name = Path::new(&asset.bin)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(asset.bin.as_str())
        .to_string();
    let resolved = if std::fs::metadata(&bin_target).is_ok() {
        bin_target.clone()
    } else {
        find_binary_in_tree(install_dir, &bin_name)
            .ok_or_else(|| InstallError::BinaryNotFound(bin_name.clone()))?
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The binary MUST be executable — chmod and treat failure as a real
        // install error (not the old best-effort skip).
        let mut perms = std::fs::metadata(&resolved)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&resolved, perms)?;
    }

    // Final gate: confirm the binary is really there before we register it.
    if std::fs::metadata(&resolved).is_err() {
        return Err(InstallError::BinaryNotFound(resolved.display().to_string()));
    }

    // Drop the staged download once extraction has succeeded.
    let _ = tokio::fs::remove_file(&staged_file).await;

    Ok(resolved)
}

/// Recursively search `dir` for a file named `name` (the extracted binary may
/// be nested inside an archive's directory layout). Returns the first match.
fn find_binary_in_tree(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip the leftover download staging dir.
                if path.file_name().and_then(|n| n.to_str()) != Some("staging") {
                    stack.push(path);
                }
            } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(path);
            }
        }
    }
    None
}

fn github_asset_url(owner: &str, repo: &str, tag: &str, file: &str) -> String {
    format!("https://github.com/{owner}/{repo}/releases/download/{tag}/{file}")
}

async fn extract_asset(
    staged: &Path,
    install_dir: &Path,
    bin_target: &Path,
    file_name: &str,
) -> Result<(), InstallError> {
    let lower = file_name.to_ascii_lowercase();
    let staged = staged.to_path_buf();
    let install_dir = install_dir.to_path_buf();
    let bin_target = bin_target.to_path_buf();

    // Archive crates are sync; hop to a blocking task so we don't stall the
    // tokio reactor on large extractions.
    tokio::task::spawn_blocking(move || -> Result<(), InstallError> {
        if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            extract_tar_gz(&staged, &install_dir)
        } else if lower.ends_with(".zip") {
            extract_zip(&staged, &install_dir)
        } else if lower.ends_with(".gz") {
            extract_gz_to(&staged, &bin_target)
        } else {
            // Treat as a raw binary; rename into the final location.
            if let Some(parent) = bin_target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&staged, &bin_target)?;
            Ok(())
        }
    })
    .await
    .map_err(|e| InstallError::ParseManifest(format!("extract join: {e}")))?
}

fn extract_tar_gz(staged: &Path, out_dir: &Path) -> Result<(), InstallError> {
    let f = std::fs::File::open(staged)?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(out_dir)?;
    Ok(())
}

fn extract_zip(staged: &Path, out_dir: &Path) -> Result<(), InstallError> {
    let f = std::fs::File::open(staged)?;
    let mut zip =
        zip::ZipArchive::new(f).map_err(|e| InstallError::Zip(e.to_string()))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| InstallError::Zip(e.to_string()))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let target = out_dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

fn extract_gz_to(staged: &Path, bin_target: &Path) -> Result<(), InstallError> {
    use std::io::Read;
    let f = std::fs::File::open(staged)?;
    let mut gz = flate2::read::GzDecoder::new(f);
    if let Some(parent) = bin_target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::File::create(bin_target)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = gz.read(&mut buf)?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut out, &buf[..n])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// platform target resolution + asset matching
// ---------------------------------------------------------------------------

/// Return the Mason-style target string for the current host. We mirror
/// Mason's naming so a Mason-translated manifest matches without remapping.
pub fn current_target() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return "linux_x64_gnu";
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return "linux_arm64_gnu";
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return "darwin_x64";
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return "darwin_arm64";
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return "win_x64";
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        return "win_arm64";
    }
    #[allow(unreachable_code)]
    {
        "unknown"
    }
}

pub fn pick_asset<'a>(
    assets: &'a [GithubAsset],
    target: &str,
) -> Option<&'a GithubAsset> {
    assets.iter().find(|a| a.target == target)
}

// ---------------------------------------------------------------------------
// symlink wiring
// ---------------------------------------------------------------------------

/// Compute the bin name the user invokes — first fall back to file stem of
/// the actual binary so we always have something to symlink under.
fn default_bin_name(manifest: &ExtensionManifest, bin_path: &Path) -> String {
    if let Some(name) = manifest.run.as_ref().and_then(|r| r.command.first()) {
        return name.clone();
    }
    bin_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| manifest.id.clone())
}

async fn link_bin(bin_path: &Path, link_name: String) -> Result<PathBuf, InstallError> {
    let bin_dir = paths::bin_dir();
    tokio::fs::create_dir_all(&bin_dir).await?;
    let link_path = bin_dir.join(&link_name);
    // Idempotent: remove any prior link/file at the target.
    let _ = tokio::fs::remove_file(&link_path).await;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(bin_path, &link_path)?;
    }
    #[cfg(windows)]
    {
        // Symlinks on Windows require elevated rights; a plain copy is
        // simpler and avoids that requirement for the common case.
        std::fs::copy(bin_path, &link_path)?;
    }
    Ok(link_path)
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(target: &str, file: &str, bin: &str) -> GithubAsset {
        GithubAsset {
            target: target.to_string(),
            file: file.to_string(),
            bin: bin.to_string(),
        }
    }

    #[test]
    fn current_target_is_known() {
        let t = current_target();
        assert!(
            matches!(
                t,
                "linux_x64_gnu"
                    | "linux_arm64_gnu"
                    | "darwin_x64"
                    | "darwin_arm64"
                    | "win_x64"
                    | "win_arm64"
                    | "unknown"
            ),
            "unexpected target string: {t}"
        );
    }

    #[test]
    fn github_url_format() {
        let url = github_asset_url("rust-lang", "rust-analyzer", "2024-01-01", "ra.gz");
        assert_eq!(
            url,
            "https://github.com/rust-lang/rust-analyzer/releases/download/2024-01-01/ra.gz"
        );
    }

    #[test]
    fn pick_asset_matches_current_target() {
        let assets = vec![
            asset("linux_x64_gnu", "linux.gz", "tool"),
            asset("linux_arm64_gnu", "linux-arm.gz", "tool"),
            asset("darwin_x64", "mac.gz", "tool"),
            asset("darwin_arm64", "mac-arm.gz", "tool"),
            asset("win_x64", "win.zip", "tool.exe"),
        ];
        let picked = pick_asset(&assets, current_target());
        if matches!(
            current_target(),
            "linux_x64_gnu"
                | "linux_arm64_gnu"
                | "darwin_x64"
                | "darwin_arm64"
                | "win_x64"
        ) {
            assert!(picked.is_some());
        }
    }

    #[test]
    fn pick_asset_returns_none_for_unknown() {
        let assets = vec![asset("linux_x64_gnu", "f", "b")];
        assert!(pick_asset(&assets, "fictional_target").is_none());
    }

    #[test]
    fn npm_bin_path_resolves_under_node_modules_dot_bin() {
        let install = Path::new("/tmp/fake-install");
        let bin_dir = install.join("node_modules").join(".bin");
        let resolved = npm_bin_path(&bin_dir, "server-filesystem");
        #[cfg(not(windows))]
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/fake-install/node_modules/.bin/server-filesystem")
        );
        #[cfg(windows)]
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/fake-install/node_modules/.bin/server-filesystem.cmd")
        );
    }

    #[test]
    fn venv_bin_path_resolves_under_venv_bin() {
        let venv = Path::new("/tmp/fake/venv");
        let resolved = venv_bin_path(venv, "black");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(resolved, PathBuf::from("/tmp/fake/venv/bin/black"));
        #[cfg(target_os = "windows")]
        assert_eq!(resolved, PathBuf::from("/tmp/fake/venv/Scripts/black.exe"));
    }

    #[test]
    fn venv_pip_path_resolves_under_venv_bin() {
        let venv = Path::new("/tmp/fake/venv");
        let resolved = venv_pip_path(venv);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(resolved, PathBuf::from("/tmp/fake/venv/bin/pip"));
        #[cfg(target_os = "windows")]
        assert_eq!(resolved, PathBuf::from("/tmp/fake/venv/Scripts/pip.exe"));
    }

    #[test]
    fn parse_percent_extracts_first_percent_number() {
        assert_eq!(parse_percent("Downloading wheel (45%) ..."), Some(45));
        assert_eq!(parse_percent("info attempt #2 [done in 50ms]"), None);
        assert_eq!(parse_percent("100% complete"), Some(100));
        assert_eq!(parse_percent("nothing"), None);
    }

    #[test]
    fn install_error_display_variants() {
        let e = InstallError::Network("boom".to_string());
        assert_eq!(format!("{e}"), "network error: boom");

        let e = InstallError::MissingTool("npm");
        assert_eq!(format!("{e}"), "tool `npm` not found on PATH");

        let e = InstallError::NoAssetForTarget("linux_x64_gnu".to_string());
        assert_eq!(format!("{e}"), "no asset matches target `linux_x64_gnu`");

        let e = InstallError::ParseManifest("oops".to_string());
        assert_eq!(format!("{e}"), "config parse error: oops");

        let e = InstallError::NotImplemented;
        assert_eq!(format!("{e}"), "not yet implemented");

        let e = InstallError::Zip("bad zip".to_string());
        assert_eq!(format!("{e}"), "zip error: bad zip");
    }

    #[test]
    fn install_kind_tag_matches_discriminant() {
        let npm = InstallKind::Npm {
            package: "x".to_string(),
            version: "1".to_string(),
        };
        assert_eq!(install_kind_tag(&npm), "npm");

        let pip = InstallKind::Pip {
            package: "x".to_string(),
            version: "1".to_string(),
        };
        assert_eq!(install_kind_tag(&pip), "pip");

        let gh = InstallKind::GithubRelease {
            owner: "o".to_string(),
            repo: "r".to_string(),
            tag: "t".to_string(),
            assets: vec![],
        };
        assert_eq!(install_kind_tag(&gh), "github_release");
    }

    #[test]
    fn forward_progress_clamps_and_uses_percent_hint() {
        let count = std::sync::Arc::new(std::sync::Mutex::new(0));
        let p = forward_progress(&count, "Downloading wheel (37%)");
        assert_eq!(p, 37);

        // Counter increments and stays under 90 even after many lines.
        let count2 = std::sync::Arc::new(std::sync::Mutex::new(0));
        let mut last = 0u8;
        for _ in 0..500 {
            last = forward_progress(&count2, "some log");
        }
        assert!(last <= 90);
    }
}
