//! Wave 6C: bidirectional CRDT cutover for editor buffers.
//!
//! Covers both sync directions plus the echo-loop guards:
//!
//! - nvim→CRDT: an `on_lines` change folds into the authoritative
//!   replica as a MINIMAL daemon-origin update (no full re-seed) that a
//!   subscribed peer can apply incrementally.
//! - CRDT→nvim: a remote client update is replayed into the live nvim
//!   buffer (gated end-to-end test, skipped when `nvim` is missing).
//! - No echo loop: a CRDT-applied remote update must NOT re-emit itself
//!   as a fresh daemon-origin update, and daemon-origin updates must
//!   never be replayed back into nvim.

use std::path::PathBuf;
use std::time::Duration;

use neoism_protocol::crdt::{
    CrdtBufferUpdate, CrdtClientMessage, CrdtServerMessage, CrdtSyncEnvelope,
};
use neoism_ui::editor::crdt::{CrdtTextBuffer, CrdtTextEdit};
use neoism_workspace_daemon::crdt::sync::{min_utf16_replace, CrdtSyncHub};
use neoism_workspace_daemon::crdt::{crdt_buffer_id_for_path, CrdtBufferRegistry};
use neoism_workspace_daemon::nvim::{remote_sync_targets_nvim, NvimSessionRegistry};

fn drain_syncs(
    rx: &mut tokio::sync::broadcast::Receiver<CrdtServerMessage>,
) -> Vec<CrdtSyncEnvelope> {
    let mut out = Vec::new();
    while let Ok(message) = rx.try_recv() {
        if let CrdtServerMessage::Sync { envelope } = message {
            out.push(envelope);
        }
    }
    out
}

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

// ---------------------------------------------------------------------
// nvim→CRDT: incremental line changes (no nvim binary required — the
// on_lines payload is simulated exactly as the lua bridge reports it).
// ---------------------------------------------------------------------

#[test]
fn nvim_lines_change_applies_minimal_daemon_origin_update() {
    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(900));
    hub.open_buffer("file:///w/a.rs", "alpha\nbravo\ncharlie");
    let peer = seed_peer_from_hub(&hub, "file:///w/a.rs", 11);
    let mut rx = hub.subscribe();

    // nvim reports: line 1 replaced ("bravo" → "bravo edited").
    let reply = hub
        .apply_nvim_lines_change(
            "file:///w/a.rs",
            1,
            2,
            1,
            "bravo edited",
            hub.daemon_client_id(),
        )
        .expect("tracked buffer accepts the change");

    assert_eq!(
        hub.buffers().text("file:///w/a.rs").unwrap(),
        "alpha\nbravo edited\ncharlie"
    );
    let CrdtServerMessage::Sync { envelope } = reply else {
        panic!("expected Sync reply, got {reply:?}");
    };
    assert_eq!(envelope.origin_client_id, hub.daemon_client_id());

    // The broadcast carries the SAME daemon-origin update, and it is
    // incremental: a peer that only had the seed converges by applying
    // just these update bytes (not a re-snapshot).
    let syncs = drain_syncs(&mut rx);
    assert_eq!(syncs.len(), 1, "exactly one Sync broadcast, got {syncs:?}");
    assert_eq!(syncs[0].origin_client_id, hub.daemon_client_id());
    peer.apply_update_v1(&syncs[0].update_v1).unwrap();
    assert_eq!(peer.text(), "alpha\nbravo edited\ncharlie");
}

#[test]
fn nvim_lines_deletion_and_insertion_edge_cases() {
    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(901));
    hub.open_buffer("file:///w/b.rs", "one\ntwo\nthree");

    // Pure deletion: on_lines reports new_line_count == 0.
    hub.apply_nvim_lines_change("file:///w/b.rs", 1, 2, 0, "", hub.daemon_client_id())
        .expect("deletion applies");
    assert_eq!(hub.buffers().text("file:///w/b.rs").unwrap(), "one\nthree");

    // Insertion at top: lines [0, 0) replaced with two new lines.
    hub.apply_nvim_lines_change(
        "file:///w/b.rs",
        0,
        0,
        2,
        "zero\nhalf",
        hub.daemon_client_id(),
    )
    .expect("insertion applies");
    assert_eq!(
        hub.buffers().text("file:///w/b.rs").unwrap(),
        "zero\nhalf\none\nthree"
    );

    // Replacing with an explicit single EMPTY line is distinct from a
    // deletion (new_line_count disambiguates the empty joined text).
    hub.apply_nvim_lines_change("file:///w/b.rs", 0, 1, 1, "", hub.daemon_client_id())
        .expect("blanking a line applies");
    assert_eq!(
        hub.buffers().text("file:///w/b.rs").unwrap(),
        "\nhalf\none\nthree"
    );

    // Untracked buffers are ignored (no panic, no broadcast).
    assert!(hub
        .apply_nvim_lines_change(
            "file:///w/missing.rs",
            0,
            1,
            1,
            "x",
            hub.daemon_client_id()
        )
        .is_none());
}

#[test]
fn nvim_lines_change_is_a_noop_when_text_already_matches() {
    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(902));
    hub.open_buffer("file:///w/c.rs", "same\ntext");
    let mut rx = hub.subscribe();

    // nvim re-reports a line that already matches the replica (e.g. a
    // formatting pass that changed nothing): no update, no broadcast.
    assert!(hub
        .apply_nvim_lines_change(
            "file:///w/c.rs",
            0,
            1,
            1,
            "same",
            hub.daemon_client_id()
        )
        .is_none());
    assert!(drain_syncs(&mut rx).is_empty());
}

#[test]
fn min_utf16_replace_trims_to_the_changed_span() {
    // Plain ASCII: only the changed middle is replaced.
    assert_eq!(
        min_utf16_replace("alpha\nbravo\ncharlie", "alpha\nbravo edited\ncharlie"),
        Some((11, 0, " edited".to_string()))
    );
    // Identical texts produce no edit.
    assert_eq!(min_utf16_replace("same", "same"), None);
    // Multibyte scalars never split, and offsets are UTF-16 units:
    // "🦀" is 4 UTF-8 bytes but 2 UTF-16 code units.
    let (index, len, content) = min_utf16_replace("a🦀b", "a🦀🦀b").expect("differs");
    assert_eq!((index, len), (3, 0));
    assert_eq!(content, "🦀");
    // Replacement across similar multibyte chars stays on char bounds.
    let (index, len, content) = min_utf16_replace("aéz", "aèz").expect("differs");
    assert_eq!((index, len), (1, 1));
    assert_eq!(content, "è");
}

// ---------------------------------------------------------------------
// CRDT→nvim routing filter (echo guard #2, pure logic).
// ---------------------------------------------------------------------

#[test]
fn remote_sync_filter_skips_daemon_origin_and_non_file_buffers() {
    let daemon_id = 9_000_000_000;
    let envelope = |origin: u64, buffer_id: &str| CrdtSyncEnvelope {
        buffer_id: buffer_id.into(),
        origin_client_id: origin,
        update_v1: vec![1],
        state_vector_v1: vec![0],
    };

    // Remote client update on a file buffer → replay into nvim.
    assert_eq!(
        remote_sync_targets_nvim(&envelope(42, "file:///w/a.rs"), daemon_id),
        Some("/w/a.rs")
    );
    // Daemon-origin update (came FROM nvim) → must never replay (echo loop).
    assert_eq!(
        remote_sync_targets_nvim(&envelope(daemon_id, "file:///w/a.rs"), daemon_id),
        None
    );
    // Non-file buffer ids have no nvim counterpart.
    assert_eq!(
        remote_sync_targets_nvim(&envelope(42, "scratch:notes"), daemon_id),
        None
    );
}

// ---------------------------------------------------------------------
// Remote client update: applied once, broadcast once, no daemon echo.
// ---------------------------------------------------------------------

#[test]
fn remote_update_broadcasts_once_and_does_not_reemit_itself() {
    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(903));
    hub.open_buffer("file:///w/d.rs", "hello");
    let peer = seed_peer_from_hub(&hub, "file:///w/d.rs", 7);
    let mut rx = hub.subscribe();

    let edit = peer
        .apply_local_edit(CrdtTextEdit::Insert {
            index: 5,
            content: " world".into(),
        })
        .unwrap();
    hub.handle_client_message(CrdtClientMessage::ApplyUpdate {
        update: CrdtBufferUpdate {
            buffer_id: "file:///w/d.rs".into(),
            origin_client_id: edit.origin_client_id,
            update_v1: edit.update_v1,
            state_vector_v1: edit.state_vector_v1,
        },
    });

    assert_eq!(hub.buffers().text("file:///w/d.rs").unwrap(), "hello world");
    let syncs = drain_syncs(&mut rx);
    assert_eq!(syncs.len(), 1, "exactly one Sync for one client update");
    assert_eq!(
        syncs[0].origin_client_id, 7,
        "the broadcast keeps the CLIENT origin; a daemon-origin re-emit \
         here would be the echo loop"
    );
}

// ---------------------------------------------------------------------
// Full bidirectional round trip through a real embedded nvim (gated).
// ---------------------------------------------------------------------

async fn read_nvim_text(
    handle: &neoism_workspace_daemon::nvim::NvimSessionHandle,
) -> Option<String> {
    handle
        .read_active_buffer()
        .await
        .ok()
        .flatten()
        .map(|buffer| buffer.text)
}

#[tokio::test]
async fn nvim_round_trip_bidirectional_sync_when_available() {
    if std::process::Command::new("nvim")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("nvim not installed; skipping bidirectional round trip");
        return;
    }

    let work = tempfile::TempDir::new().unwrap();
    let file = work.path().join("shared.txt");
    std::fs::write(&file, "alpha\nbravo\ncharlie\n").unwrap();
    let abs = file.canonicalize().unwrap();

    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(904));
    let registry = NvimSessionRegistry::new();
    let handle = registry
        .get_or_spawn("test:bidi".into(), &hub)
        .await
        .expect("spawn nvim");

    handle
        .handle(neoism_protocol::editor::EditorClientMessage::OpenBuffer {
            path: PathBuf::from(&abs),
            line: None,
            character: None,
            surface_id: Some("test:bidi".into()),
        })
        .await
        .expect("open buffer");
    // Same seed path the websocket Editor arm runs after OpenBuffer.
    neoism_workspace_daemon::server::seed_crdt_from_open_buffer(&hub, &handle, 0).await;

    let buffer_id = crdt_buffer_id_for_path(&abs);
    assert_eq!(
        hub.buffers().text(&buffer_id).unwrap(),
        "alpha\nbravo\ncharlie",
        "seed matches nvim's view of the file"
    );

    let mut rx = hub.subscribe();
    let remote = seed_peer_from_hub(&hub, &buffer_id, 77);

    // ---- CRDT→nvim: a remote web edit lands in the live buffer ----
    let edit = remote
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

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if read_nvim_text(&handle).await.as_deref()
            == Some("REMOTE alpha\nbravo\ncharlie")
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "remote CRDT update never reached the nvim buffer; nvim text = {:?}",
            read_nvim_text(&handle).await
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // ---- echo guard: the applied remote op must not re-emit ----
    // Give the (suppressed) on_lines window time to fire if the guard
    // were broken, then assert no daemon-origin Sync appeared.
    tokio::time::sleep(Duration::from_millis(700)).await;
    let syncs = drain_syncs(&mut rx);
    assert_eq!(
        syncs.iter().filter(|s| s.origin_client_id == 77).count(),
        1,
        "the remote edit broadcasts once"
    );
    assert!(
        !syncs
            .iter()
            .any(|s| s.origin_client_id == hub.daemon_client_id()),
        "echo loop: applying the remote op into nvim re-emitted it as a \
         daemon-origin update: {syncs:?}"
    );

    // ---- nvim→CRDT: a local nvim edit streams into the doc ----
    handle
        .handle(neoism_protocol::editor::EditorClientMessage::SendKeys {
            bytes: b"GoNVIMLINE<Esc>".to_vec(),
            surface_id: Some("test:bidi".into()),
        })
        .await
        .expect("send keys");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if hub
            .buffers()
            .text(&buffer_id)
            .unwrap_or_default()
            .contains("NVIMLINE")
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "nvim edit never reached the CRDT doc; doc text = {:?}",
            hub.buffers().text(&buffer_id)
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // The nvim edit broadcast as daemon-origin incremental updates the
    // remote replica converges from — WITHOUT requesting a snapshot.
    tokio::time::sleep(Duration::from_millis(300)).await;
    for envelope in drain_syncs(&mut rx) {
        if envelope.origin_client_id != remote.client_id() {
            remote.apply_update_v1(&envelope.update_v1).unwrap();
        }
    }
    assert_eq!(
        remote.text(),
        hub.buffers().text(&buffer_id).unwrap(),
        "remote replica converges from broadcast updates alone"
    );
    assert_eq!(
        read_nvim_text(&handle).await.as_deref(),
        Some(hub.buffers().text(&buffer_id).unwrap().as_str()),
        "nvim and the authoritative doc agree after edits from both sides"
    );

    let _ = handle
        .handle(neoism_protocol::editor::EditorClientMessage::Close)
        .await;
    registry.remove(handle.key()).await;
}

// ---------------------------------------------------------------------
// Two screens, two embedded nvim sessions, ONE document: typing in one
// session must land in the other. This is the exact web+desktop-on-the-
// same-file scenario — it broke when every session stamped the shared
// daemon id and each applier (skipping "its own" origin) swallowed all
// nvim edits, its own and every peer session's alike.
// ---------------------------------------------------------------------

#[tokio::test]
async fn nvim_to_nvim_cross_session_sync_when_available() {
    if std::process::Command::new("nvim")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("nvim not installed; skipping cross-session round trip");
        return;
    }

    let work = tempfile::TempDir::new().unwrap();
    let file = work.path().join("shared.txt");
    std::fs::write(&file, "alpha\nbravo\n").unwrap();
    let abs = file.canonicalize().unwrap();

    let hub = CrdtSyncHub::new(CrdtBufferRegistry::with_daemon_client_id(905));
    let registry = NvimSessionRegistry::new();
    let session_a = registry
        .get_or_spawn("screen:a".into(), &hub)
        .await
        .expect("spawn nvim A");
    let session_b = registry
        .get_or_spawn("screen:b".into(), &hub)
        .await
        .expect("spawn nvim B");

    for (session, surface) in [(&session_a, "screen:a"), (&session_b, "screen:b")] {
        session
            .handle(neoism_protocol::editor::EditorClientMessage::OpenBuffer {
                path: PathBuf::from(&abs),
                line: None,
                character: None,
                surface_id: Some(surface.into()),
            })
            .await
            .expect("open buffer");
        neoism_workspace_daemon::server::seed_crdt_from_open_buffer(&hub, session, 0)
            .await;
    }

    // Type in A: insert at the top of the buffer, leave insert mode.
    session_a
        .handle(neoism_protocol::editor::EditorClientMessage::SendKeys {
            bytes: b"ggiHELLO <Esc>".to_vec(),
            surface_id: Some("screen:a".into()),
        })
        .await
        .expect("type in A");

    // B's buffer must converge on A's keystrokes via the hub.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let text = session_b
            .read_active_buffer()
            .await
            .ok()
            .flatten()
            .map(|buffer| buffer.text);
        if text.as_deref() == Some("HELLO alpha\nbravo") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "A's typing never reached B's nvim buffer; B = {text:?} hub = {:?}",
            hub.buffers().text(&crdt_buffer_id_for_path(&abs))
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // And the reverse direction: B types, A converges.
    session_b
        .handle(neoism_protocol::editor::EditorClientMessage::SendKeys {
            bytes: b"GAend<Esc>".to_vec(),
            surface_id: Some("screen:b".into()),
        })
        .await
        .expect("type in B");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let text = session_a
            .read_active_buffer()
            .await
            .ok()
            .flatten()
            .map(|buffer| buffer.text);
        if text.as_deref() == Some("HELLO alpha\nbravoend") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "B's typing never reached A's nvim buffer; A = {text:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
