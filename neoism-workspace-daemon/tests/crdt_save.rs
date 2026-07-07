//! Daemon-owned save: every editor's "write" flushes the AUTHORITATIVE
//! converged document to disk through the daemon (single writer).
//!
//! - `SaveBuffer` writes the hub's doc text (not any client's buffer)
//!   and broadcasts `Saved` so every client can clear its doc-level
//!   dirty bit.
//! - nvim's `:w` is intercepted by the per-buffer `BufWriteCmd`
//!   autocmd (attached alongside the on_lines bridge) and funnels into
//!   the same flush — gated end-to-end test, skipped without `nvim`.

use std::path::PathBuf;
use std::time::Duration;

use neoism_protocol::crdt::{CrdtBufferUpdate, CrdtClientMessage, CrdtServerMessage};
use neoism_ui::editor::crdt::{CrdtTextBuffer, CrdtTextEdit};
use neoism_workspace_daemon::crdt::sync::CrdtSyncHub;
use neoism_workspace_daemon::crdt::{crdt_buffer_id_for_path, CrdtBufferRegistry};
use neoism_workspace_daemon::nvim::NvimSessionRegistry;

fn seed_peer_from_hub(
    hub: &CrdtSyncHub,
    buffer_id: &str,
    client_id: u64,
) -> CrdtTextBuffer {
    let peer = CrdtTextBuffer::new(client_id);
    let snapshot = hub
        .buffers()
        .snapshot_for(buffer_id, &[])
        .expect("seeded buffer snapshots");
    peer.apply_update_v1(&snapshot.update_v1)
        .expect("snapshot applies");
    peer
}

#[test]
fn save_buffer_writes_converged_doc_and_broadcasts_saved() {
    let work = tempfile::TempDir::new().unwrap();
    let file = work.path().join("note.md");
    std::fs::write(&file, "stale on-disk text").unwrap();

    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(910));
    let buffer_id = crdt_buffer_id_for_path(&file);
    hub.open_buffer(buffer_id.clone(), "hello");

    // A peer's accepted edit joins the doc before the save.
    let peer = seed_peer_from_hub(&hub, &buffer_id, 31);
    let edit = peer
        .apply_local_edit(CrdtTextEdit::Insert {
            index: 5,
            content: " world".into(),
        })
        .unwrap();
    hub.handle_client_message(CrdtClientMessage::ApplyUpdate {
        update: CrdtBufferUpdate {
            buffer_id: buffer_id.clone(),
            origin_client_id: edit.origin_client_id,
            update_v1: edit.update_v1,
            state_vector_v1: edit.state_vector_v1,
        },
    });

    let mut rx = hub.subscribe();
    let replies = hub.handle_client_message(CrdtClientMessage::SaveBuffer {
        buffer_id: buffer_id.clone(),
    });

    // Reply confirms the converged write...
    assert_eq!(
        replies,
        vec![CrdtServerMessage::Saved {
            buffer_id: buffer_id.clone(),
            bytes_written: "hello world".len() as u64,
        }]
    );
    // ...the file holds the DOC text (peer's edit included, stale disk
    // bytes gone)...
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello world");
    // ...and every subscriber hears `Saved` (doc-level dirty bit).
    let mut saw_saved = false;
    while let Ok(message) = rx.try_recv() {
        if matches!(&message, CrdtServerMessage::Saved { buffer_id: id, .. } if *id == buffer_id)
        {
            saw_saved = true;
        }
    }
    assert!(saw_saved, "Saved must broadcast to all subscribers");
}

#[test]
fn save_buffer_rejects_untracked_and_non_file_ids() {
    let hub = CrdtSyncHub::default();

    // Non-file scheme: nothing to write.
    hub.open_buffer("scratch:notes", "text");
    match hub
        .handle_client_message(CrdtClientMessage::SaveBuffer {
            buffer_id: "scratch:notes".into(),
        })
        .remove(0)
    {
        CrdtServerMessage::Error { message, .. } => {
            assert!(message.contains("no file backing"), "{message}");
        }
        other => panic!("expected error, got {other:?}"),
    }

    // Untracked buffer: unknown to the registry.
    match hub
        .handle_client_message(CrdtClientMessage::SaveBuffer {
            buffer_id: "file:///nowhere/untracked.md".into(),
        })
        .remove(0)
    {
        CrdtServerMessage::Error { message, .. } => {
            assert!(message.contains("unknown CRDT buffer"), "{message}");
        }
        other => panic!("expected error, got {other:?}"),
    }
}

#[tokio::test]
async fn nvim_write_interception_flushes_daemon_doc_when_available() {
    if std::process::Command::new("nvim")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("nvim not installed; skipping :w interception round trip");
        return;
    }

    let work = tempfile::TempDir::new().unwrap();
    let file = work.path().join("shared.txt");
    std::fs::write(&file, "alpha\nbravo\n").unwrap();
    let abs = file.canonicalize().unwrap();

    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(911));
    let registry = NvimSessionRegistry::new();
    let handle = registry
        .get_or_spawn("test:save".into(), &hub)
        .await
        .expect("spawn nvim");

    handle
        .handle(neoism_protocol::editor::EditorClientMessage::OpenBuffer {
            path: PathBuf::from(&abs),
            line: None,
            character: None,
            surface_id: Some("test:save".into()),
        })
        .await
        .expect("open buffer");
    neoism_workspace_daemon::server::seed_crdt_from_open_buffer(&hub, &handle, 0).await;

    let buffer_id = crdt_buffer_id_for_path(&abs);
    assert_eq!(hub.buffers().text(&buffer_id).unwrap(), "alpha\nbravo");

    // A remote peer (web/desktop markdown) edits the doc; the CRDT→nvim
    // applier replays it into the live buffer.
    let peer = seed_peer_from_hub(&hub, &buffer_id, 41);
    let edit = peer
        .apply_local_edit(CrdtTextEdit::Insert {
            index: 0,
            content: "REMOTE ".into(),
        })
        .unwrap();
    hub.handle_client_message(CrdtClientMessage::ApplyUpdate {
        update: CrdtBufferUpdate {
            buffer_id: buffer_id.clone(),
            origin_client_id: edit.origin_client_id,
            update_v1: edit.update_v1,
            state_vector_v1: edit.state_vector_v1,
        },
    });

    // Wait for the replay to land in nvim before writing.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let text = handle
            .read_active_buffer()
            .await
            .ok()
            .flatten()
            .map(|buffer| buffer.text);
        if text.as_deref() == Some("REMOTE alpha\nbravo") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "remote edit never reached nvim; text = {text:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // `:w` — BufWriteCmd must intercept (no direct nvim write) and the
    // daemon flushes the converged doc, broadcasting `Saved`.
    let mut rx = hub.subscribe();
    handle
        .handle(neoism_protocol::editor::EditorClientMessage::SendKeys {
            bytes: b":w\r".to_vec(),
            surface_id: Some("test:save".into()),
        })
        .await
        .expect("send :w");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(CrdtServerMessage::Saved { buffer_id: id, .. })) if id == buffer_id => {
                break;
            }
            Ok(Ok(_)) => continue,
            Ok(Err(err)) => panic!("hub broadcast closed: {err}"),
            Err(_) => panic!(
                ":w never produced a daemon Saved broadcast; \
                 BufWriteCmd interception is not firing"
            ),
        }
    }
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "REMOTE alpha\nbravo",
        "the daemon wrote the CONVERGED doc text"
    );
}
