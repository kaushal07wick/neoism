use std::collections::HashMap;

use super::*;

#[test]
fn discovers_shells_from_env_and_candidates_without_duplicates() {
    let shells = discover_shells_with(
        Some("/bin/bash"),
        ["/bin/bash", "/bin/sh", "/usr/bin/fish"],
        |path| path != "/usr/bin/fish",
    );

    assert_eq!(shells.len(), 3);
    assert_eq!(shells[0].path, "/bin/bash");
    assert_eq!(shells[0].name, "bash");
    assert!(shells[0].acceptable);
    assert_eq!(shells[2].path, "/usr/bin/fish");
    assert!(!shells[2].acceptable);
}

#[test]
fn creates_pty_info_with_defaults_and_sanitized_command() {
    let info = create_pty_info(
        PtyCreateRequest {
            command: Some(vec![
                "".to_string(),
                "  /bin/bash  ".to_string(),
                "-l".to_string(),
            ]),
            cwd: Some("  ".to_string()),
            title: Some("  Terminal 1 ".to_string()),
        },
        "/workspace",
        "/bin/sh",
        42,
    );

    assert!(info.id.starts_with("pty"));
    assert_eq!(info.command, vec!["/bin/bash", "-l"]);
    assert_eq!(info.cwd, "/workspace");
    assert_eq!(info.title, "Terminal 1");
    assert_eq!(info.time, 42);
}

#[test]
fn default_pty_uses_login_shell_when_supported() {
    let bash = create_pty_info(PtyCreateRequest::default(), "/workspace", "/bin/bash", 1);
    assert_eq!(bash.command, vec!["/bin/bash", "-l"]);

    let sh = create_pty_info(PtyCreateRequest::default(), "/workspace", "/bin/sh", 1);
    assert_eq!(sh.command, vec!["/bin/sh"]);
}

#[test]
fn updates_lists_gets_and_removes_pty_info() {
    let mut ptys = HashMap::new();
    let mut first = create_pty_info(PtyCreateRequest::default(), "/a", "/bin/sh", 20);
    first.id = "pty_b".to_string();
    let mut second = create_pty_info(PtyCreateRequest::default(), "/b", "/bin/bash", 10);
    second.id = "pty_a".to_string();

    insert_pty(&mut ptys, first);
    insert_pty(&mut ptys, second);

    assert_eq!(
        list_ptys(&ptys)
            .into_iter()
            .map(|info| info.id)
            .collect::<Vec<_>>(),
        vec!["pty_a", "pty_b"]
    );

    let updated = update_pty(
        &mut ptys,
        "pty_b",
        PtyUpdateRequest {
            title: Some("  Logs ".to_string()),
            cwd: Some("/repo".to_string()),
            size: Some(PtySize {
                cols: 120,
                rows: 40,
            }),
        },
    )
    .expect("pty should update");

    assert_eq!(updated.title, "Logs");
    assert_eq!(updated.cwd, "/repo");
    assert_eq!(get_pty(&ptys, "pty_b").expect("pty exists").title, "Logs");
    assert_eq!(
        remove_pty(&mut ptys, "pty_b").expect("pty removed").id,
        "pty_b"
    );
    assert!(matches!(get_pty(&ptys, "pty_b"), Err(PtyError::NotFound)));
}

#[test]
fn validates_connect_tickets_once_and_checks_expiry_and_pty() {
    let mut tokens = ConnectTokens::default();
    let issued = tokens.issue_with_ticket("pty_1", "ticket_1", 100, 1);

    assert_eq!(
        tokens
            .validate("pty_1", "ticket_1", 199)
            .expect("valid ticket"),
        issued
    );
    assert_eq!(
        tokens.validate("pty_1", "ticket_1", 199),
        Err(PtyError::InvalidTicket)
    );

    tokens.issue_with_ticket("pty_1", "ticket_2", 100, 1);
    assert_eq!(
        tokens.validate("pty_2", "ticket_2", 199),
        Err(PtyError::TicketPtyMismatch)
    );

    tokens.issue_with_ticket("pty_1", "ticket_3", 100, 1);
    assert_eq!(
        tokens.validate("pty_1", "ticket_3", 1100),
        Err(PtyError::ExpiredTicket)
    );
}

#[test]
fn prunes_expired_connect_tickets() {
    let mut tokens = ConnectTokens::default();
    tokens.issue_with_ticket("pty_1", "expired", 0, 1);
    tokens.issue_with_ticket("pty_1", "active", 0, 2);

    tokens.prune_expired(1000);

    assert_eq!(
        tokens.validate("pty_1", "expired", 1000),
        Err(PtyError::InvalidTicket)
    );
    assert!(tokens.validate("pty_1", "active", 1000).is_ok());
}

#[test]
fn output_buffer_replays_from_cursor() {
    let mut buffer = PtyOutputBuffer::default();
    let first = buffer.push("abc".to_string());
    let second = buffer.push("de".to_string());

    assert_eq!(first.cursor, 3);
    assert_eq!(second.cursor, 5);
    assert_eq!(
        buffer
            .replay_from(2)
            .into_iter()
            .map(|chunk| chunk.data)
            .collect::<Vec<_>>(),
        vec!["c", "de"]
    );
    assert!(buffer.replay_from(5).is_empty());
}

#[test]
fn output_buffer_uses_utf16_cursor_offsets() {
    let mut buffer = PtyOutputBuffer::default();
    buffer.push("a😀b".to_string());

    assert_eq!(buffer.cursor(), 4);
    assert_eq!(buffer.replay_from(3)[0].data, "b");
}
