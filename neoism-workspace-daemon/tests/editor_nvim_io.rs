//! Reproduction of the desktop's "open a code file" path against the
//! daemon's editor websocket surface: send `Resize` + `OpenBuffer` for a
//! real file and assert the daemon spawns `nvim --embed` and streams the
//! buffer's text back as redraw (`EditorReply`).
//!
//! If this PASSES, the daemon nvim path works and a blank editor in the
//! app is a CLIENT/connection problem (the desktop never delivering the
//! messages), not the daemon. If it FAILS, the failure point is right here.

use std::net::SocketAddr;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use neoism_workspace_daemon::auth::AuthService;
use neoism_workspace_daemon::handshake::PairingTokenStore;
use neoism_workspace_daemon::nvim::NvimSessionRegistry;
use neoism_workspace_daemon::server::{self, AppState};
use neoism_workspace_daemon::workspace::WorkspaceManager;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn boot() -> (SocketAddr, tokio::task::JoinHandle<()>, TempDir) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("neoism_workspace_daemon=info,neoism_backend=info")
        .with_test_writer()
        .try_init();
    std::env::remove_var("NEOISM_REQUIRE_AUTH");
    let data = TempDir::new().unwrap();
    std::env::set_var("NEOISM_DAEMON_DATA_DIR", data.path());
    let auth = AuthService::bootstrap(data.path()).unwrap();
    let workspaces = WorkspaceManager::bootstrap();
    let pairing = PairingTokenStore::in_memory();
    let app = server::router(AppState {
        auth,
        sessions: neoism_workspace_daemon::sessions::SessionRegistry::shared(),
        workspaces,
        pairing_tokens: pairing,
        nvim_sessions: NvimSessionRegistry::new(),
        crdt: neoism_workspace_daemon::crdt::sync::CrdtSyncHub::default(),
        paired_hosts: neoism_workspace_daemon::hosts::PairedHostStore::in_memory(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    (addr, task, data)
}

#[tokio::test]
async fn daemon_opens_file_in_nvim_and_streams_redraw() {
    if std::process::Command::new("nvim")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("nvim not installed; skipping");
        return;
    }

    let (addr, task, _data) = boot().await;

    // A real file with a unique marker we expect to see in the redraw grid.
    let work = TempDir::new().unwrap();
    let file = work.path().join("hello.rs");
    std::fs::write(
        &file,
        "LINEALPHA first\nLINEBRAVO second\nMARKERNVIMREPRO third\nLINEDELTA fourth\n",
    )
    .unwrap();
    // Match the DESKTOP exactly: workspace_root=null + ABSOLUTE path.
    let abs_path = file.to_string_lossy().to_string();
    eprintln!("REPRO: opening abs file {abs_path}");

    let url = format!("ws://{addr}/session");
    let (mut ws, _) = connect_async(&url).await.expect("ws upgrade");

    let surface = "s1";
    let resize = serde_json::json!({"Editor": {"request_id": 1, "workspace_root": serde_json::Value::Null,
        "message": {"Resize": {"width": 80, "height": 24, "surface_id": surface}}}});
    let open = serde_json::json!({"Editor": {"request_id": 2, "workspace_root": serde_json::Value::Null,
        "message": {"OpenBuffer": {"path": abs_path, "surface_id": surface}}}});
    ws.send(Message::Text(resize.to_string())).await.unwrap();
    ws.send(Message::Text(open.to_string())).await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut editor_replies = 0usize;
    let mut error_msg: Option<String> = None;
    // Reconstruct the grid text from `GridUpdate` cells. Each cell serialises
    // as `{"row":R,"col":C,"ch":"X",...}` — so the line text is NOT contiguous
    // in the JSON; we pull every `"ch":"X"` out and concatenate.
    let mut grid_text = String::new();
    while tokio::time::Instant::now() < deadline {
        let frame =
            match tokio::time::timeout(Duration::from_millis(500), ws.next()).await {
                Ok(Some(Ok(Message::Text(t)))) => t,
                Ok(Some(Ok(_))) => continue,
                Ok(Some(Err(_))) | Ok(None) => break,
                Err(_) => continue,
            };
        let v: serde_json::Value = match serde_json::from_str(&frame) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(reply) = v.get("EditorReply") {
            editor_replies += 1;
            let s = reply.to_string();
            if s.contains("\"Error\"") {
                error_msg = Some(s.clone());
            }
            let mut idx = 0;
            while let Some(pos) = s[idx..].find("\"ch\":\"") {
                let start = idx + pos + 6;
                if let Some(end_rel) = s[start..].find('"') {
                    grid_text.push_str(&s[start..start + end_rel]);
                    idx = start + end_rel;
                } else {
                    break;
                }
            }
        }
    }
    task.abort();

    let saw_marker = grid_text.contains("MARKERNVIMREPRO");
    eprintln!(
        "REPRO RESULT: editor_replies={editor_replies} grid_text_len={} saw_marker={saw_marker} error={error_msg:?}",
        grid_text.len()
    );

    assert!(
        error_msg.is_none(),
        "daemon returned an editor Error: {error_msg:?}"
    );
    assert!(
        saw_marker,
        "the file's text never reached the forwarded grid — daemon editor redraw is broken (replies={editor_replies}, grid_text={grid_text:?})"
    );
}
