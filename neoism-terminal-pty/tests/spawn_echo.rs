//! Smoke test for `PtySession::spawn`.
//!
//! Spawns `sh -c 'printf hello'` behind a fresh PTY and checks that
//! `b"hello"` shows up in the read stream within ~1s.
//!
//! The test is `#[ignore]` because:
//!   * It depends on `/bin/sh` (and `printf`) being present and
//!     behaving in the test environment.
//!   * `teletypewriter` writes via spawn, so on some sandboxes the
//!     fork can race with the master fd setup and produce 0 bytes
//!     before the child writes. The test loops with a wall-clock cap
//!     to stay deterministic-ish, but is still timing-sensitive on
//!     loaded CI runners.
//! Run with `cargo test -p neoism-terminal-pty -- --ignored`.

#![cfg(unix)]

use neoism_terminal_pty::{PtySession, PtySessionConfig};
use std::time::{Duration, Instant};

#[test]
#[ignore]
fn spawn_echo_emits_hello() {
    let config = PtySessionConfig {
        shell: Some("/bin/sh".to_string()),
        args: vec!["-c".to_string(), "printf hello".to_string()],
        cwd: None,
        env: Vec::new(),
        cols: 80,
        rows: 24,
    };
    let mut session = PtySession::spawn(config).expect("spawn PTY");

    let deadline = Instant::now() + Duration::from_secs(1);
    let mut got = Vec::<u8>::new();
    let mut buf = [0u8; 256];
    while Instant::now() < deadline {
        match session.read(&mut buf) {
            Ok(0) => {
                if session.exit_code().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => panic!("read failed: {err}"),
        }
        if got.windows(5).any(|w| w == b"hello") {
            break;
        }
    }

    session.close();

    assert!(
        got.windows(5).any(|w| w == b"hello"),
        "expected to see `hello` in PTY output, got: {got:?}"
    );
}
