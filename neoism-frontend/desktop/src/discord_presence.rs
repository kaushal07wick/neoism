use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use discord_presence::models::Event;
use discord_presence::Client;
use tracing::{debug, warn};

const ENABLE_ENV: &str = "NEOISM_DISCORD_PRESENCE";
const APP_ID_ENV: &str = "NEOISM_DISCORD_APP_ID";
const ASSET_KEY_ENV: &str = "NEOISM_DISCORD_ASSET_KEY";
const DEFAULT_APP_ID: u64 = 1_517_226_610_586_292_345;
const DEFAULT_LOGO_ASSET_KEY: &str =
    "72fa3f57457bbfa2577fac928e9bd577316e722e8a639732c5f8e8c58ba0e9d4";
const CONNECTION_ATTEMPTS: usize = 3;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(250);
const PROJECT_URL: &str = env!("CARGO_PKG_REPOSITORY");

pub fn start() {
    match configured_enabled() {
        Some(false) => {
            debug!("Discord presence disabled by {ENABLE_ENV}");
            return;
        }
        Some(true) => {}
        None if !discord_ipc_available() => {
            debug!("Discord presence skipped because Discord IPC is unavailable");
            return;
        }
        None => {}
    }

    let client_id = configured_client_id();
    let logo_asset_key = configured_asset_key();

    if let Err(error) = thread::Builder::new()
        .name("neoism-discord-presence".to_string())
        .spawn(move || run(client_id, logo_asset_key))
    {
        warn!(?error, "failed to start Discord presence thread");
    }
}

fn configured_enabled() -> Option<bool> {
    let raw = std::env::var(ENABLE_ENV).ok()?;
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => {
            warn!(value = %raw, "ignoring invalid {ENABLE_ENV}");
            None
        }
    }
}

fn configured_client_id() -> u64 {
    let Some(raw) = std::env::var(APP_ID_ENV).ok() else {
        return DEFAULT_APP_ID;
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_APP_ID;
    }

    match trimmed.parse() {
        Ok(client_id) => client_id,
        Err(error) => {
            warn!(?error, "ignoring invalid {APP_ID_ENV}");
            DEFAULT_APP_ID
        }
    }
}

fn configured_asset_key() -> String {
    std::env::var(ASSET_KEY_ENV)
        .ok()
        .map(|asset_key| asset_key.trim().to_string())
        .filter(|asset_key| !asset_key.is_empty())
        .unwrap_or_else(|| DEFAULT_LOGO_ASSET_KEY.to_string())
}

#[cfg(unix)]
fn discord_ipc_available() -> bool {
    use std::os::unix::fs::FileTypeExt;
    use std::path::{Path, PathBuf};

    fn dir_has_socket(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return false;
            };
            if !name.starts_with("discord-ipc-") {
                return false;
            }
            entry
                .file_type()
                .map(|file_type| file_type.is_socket())
                .unwrap_or(false)
        })
    }

    let mut dirs = Vec::new();
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        dirs.push(runtime_dir.clone());
        dirs.push(runtime_dir.join("app/com.discordapp.Discord"));
        dirs.push(runtime_dir.join("snap.discord"));
    }
    dirs.push(std::env::temp_dir());

    dirs.iter().any(|dir| dir_has_socket(dir))
}

#[cfg(not(unix))]
fn discord_ipc_available() -> bool {
    true
}

fn run(client_id: u64, logo_asset_key: String) {
    let mut client = Client::with_error_config(
        client_id,
        CONNECT_RETRY_DELAY,
        Some(CONNECTION_ATTEMPTS),
    );
    client.start();

    if let Err(error) = client.block_until_event(Event::Ready) {
        debug!(?error, "Discord presence unavailable");
        return;
    }

    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();

    if let Err(error) = client.set_activity(|activity| {
        activity
            .details("Building in Neoism")
            .state("Terminal-first editor workspace")
            .timestamps(|timestamps| timestamps.start(started_at))
            .assets(|assets| assets.large_image(logo_asset_key).large_text("Neoism"))
            .append_buttons(|button| button.label("Open Neoism").url(PROJECT_URL))
    }) {
        debug!(?error, "failed to set Discord presence");
    }
}
