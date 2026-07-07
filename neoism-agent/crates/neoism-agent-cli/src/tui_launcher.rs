use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

pub(crate) struct TuiOptions {
    pub(crate) server: String,
    pub(crate) session: Option<String>,
    pub(crate) dir: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) agent: Option<String>,
    pub(crate) variant: Option<String>,
}

pub(crate) async fn run(options: TuiOptions) -> anyhow::Result<()> {
    let app_dir = opentui_app_dir()?;
    let entry = app_dir.join("src/index.ts");
    if !entry.exists() {
        anyhow::bail!("Neoism OpenTUI entrypoint is missing: {}", entry.display());
    }
    let bun = std::env::var("BUN").unwrap_or_else(|_| "bun".to_string());
    if !app_dir.join("node_modules/@opentui/core").exists() {
        install_dependencies(&bun, &app_dir)?;
    }

    let mut command = Command::new(bun);
    command
        .current_dir(&app_dir)
        .env("BUN_TMPDIR", "/tmp/neoism-bun")
        .arg("run")
        .arg("src/index.ts")
        .arg("--server")
        .arg(options.server);
    push_opt(&mut command, "--session", options.session.as_deref());
    push_opt(&mut command, "--dir", options.dir.as_deref());
    push_opt(&mut command, "--model", options.model.as_deref());
    push_opt(&mut command, "--provider", options.provider.as_deref());
    push_opt(&mut command, "--agent", options.agent.as_deref());
    push_opt(&mut command, "--variant", options.variant.as_deref());

    let status = command.status().with_context(|| {
        format!(
            "failed to start Bun for Neoism OpenTUI app in {}",
            app_dir.display()
        )
    })?;
    if !status.success() {
        anyhow::bail!("Neoism OpenTUI exited with status {status}");
    }
    Ok(())
}

fn install_dependencies(bun: &str, app_dir: &Path) -> anyhow::Result<()> {
    eprintln!(
        "installing Neoism OpenTUI dependencies in {}",
        app_dir.display()
    );
    let status = Command::new(bun)
        .current_dir(app_dir)
        .env("BUN_TMPDIR", "/tmp/neoism-bun")
        .arg("install")
        .status()
        .with_context(|| {
            format!("failed to run `bun install` in {}", app_dir.display())
        })?;
    if !status.success() {
        anyhow::bail!("`bun install` failed with status {status}");
    }
    Ok(())
}

fn push_opt(command: &mut Command, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        command.arg(key).arg(value);
    }
}

fn opentui_app_dir() -> anyhow::Result<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let app_dir = manifest_dir.join("../../apps/neoism-opentui");
    app_dir
        .canonicalize()
        .with_context(|| format!("failed to locate {}", app_dir.display()))
}
