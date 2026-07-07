//! Daemon-hosted PTY adapter.
//!
//! `RemotePty` presents the same surface as [`crate::local::LocalPty`]
//! but no process lives behind it locally: the shell runs inside the
//! workspace daemon and this adapter only moves bytes.
//!
//! * OUTPUT: the host (desktop daemon link) calls
//!   [`RemotePtyFeed::push_output`] with each daemon `PtyOutput` frame;
//!   the bytes land on the same `corcovado::channel` a local reader
//!   thread would fill, so `neoism-backend::performer::Machine`'s poll
//!   loop consumes them unchanged.
//! * INPUT/RESIZE/CLOSE: forwarded to a caller-provided sink as
//!   [`RemotePtyOp`]s — the host translates them into daemon
//!   `PtyInput` / `Resize` / `ClosePty` messages.
//! * EXIT: [`RemotePtyFeed::child_exited`] mirrors a daemon `PtyClosed`
//!   into the child-event channel + the shared exit status, exactly
//!   like a local waitpid.
//!
//! This is the "one shell, many screens" keystone: with this adapter a
//! desktop terminal tab renders the SAME daemon session a web client
//! attaches to, instead of a private local shell.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use corcovado::channel::{self, Receiver, Sender};

/// Sentinel mirroring `local.rs` — "child is still running."
const EXIT_RUNNING: i32 = i32::MIN;

/// Control operations a remote PTY forwards to its host sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemotePtyOp {
    /// Keystrokes / paste bytes destined for the daemon shell.
    Input(Vec<u8>),
    /// Viewport geometry change.
    Resize { cols: u16, rows: u16 },
    /// The tab/session is being torn down on this client.
    Close,
}

/// Host-side handle for feeding daemon output into a [`RemotePty`].
/// Cloneable so the daemon link's fan-in can hold it wherever frames
/// arrive.
#[derive(Clone)]
pub struct RemotePtyFeed {
    byte_tx: Sender<Vec<u8>>,
    child_event_tx: Sender<i32>,
    exit_status: Arc<AtomicI32>,
}

impl RemotePtyFeed {
    /// Deliver one daemon `PtyOutput` frame. Returns `false` when the
    /// consuming machine is gone (tab closed) so callers can drop the
    /// feed.
    pub fn push_output(&self, bytes: Vec<u8>) -> bool {
        if bytes.is_empty() {
            return true;
        }
        self.byte_tx.send(bytes).is_ok()
    }

    /// Mirror a daemon `PtyClosed` as a local child exit.
    pub fn child_exited(&self, exit_code: i32) {
        self.exit_status.store(exit_code, Ordering::SeqCst);
        let _ = self.child_event_tx.send(exit_code);
    }
}

/// Daemon-hosted PTY worker. See module docs.
pub struct RemotePty {
    sink: Box<dyn FnMut(RemotePtyOp) + Send>,
    byte_rx: Option<Receiver<Vec<u8>>>,
    child_event_rx: Option<Receiver<i32>>,
    exit_status: Arc<AtomicI32>,
    #[cfg(unix)]
    pub(crate) main_fd: Arc<libc::c_int>,
}

impl RemotePty {
    pub(crate) fn new(sink: Box<dyn FnMut(RemotePtyOp) + Send>) -> (Self, RemotePtyFeed) {
        let (byte_tx, byte_rx) = channel::channel::<Vec<u8>>();
        let (child_event_tx, child_event_rx) = channel::channel::<i32>();
        let exit_status = Arc::new(AtomicI32::new(EXIT_RUNNING));
        let feed = RemotePtyFeed {
            byte_tx,
            child_event_tx,
            exit_status: exit_status.clone(),
        };
        (
            Self {
                sink,
                byte_rx: Some(byte_rx),
                child_event_rx: Some(child_event_rx),
                exit_status,
                #[cfg(unix)]
                // No local fd exists — `-1` keeps the
                // foreground-process introspection helpers safely inert
                // (they treat invalid fds as "no info").
                main_fd: Arc::new(-1),
            },
            feed,
        )
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        (self.sink)(RemotePtyOp::Input(bytes.to_vec()));
        Ok(bytes.len())
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) -> std::io::Result<()> {
        (self.sink)(RemotePtyOp::Resize { cols, rows });
        Ok(())
    }

    pub(crate) fn close(mut self) {
        (self.sink)(RemotePtyOp::Close);
    }

    pub(crate) fn exit_code(&self) -> Option<i32> {
        match self.exit_status.load(Ordering::SeqCst) {
            EXIT_RUNNING => None,
            status => Some(status),
        }
    }

    pub(crate) fn take_byte_receiver(&mut self) -> Option<Receiver<Vec<u8>>> {
        self.byte_rx.take()
    }

    pub(crate) fn take_child_event_receiver(&mut self) -> Option<Receiver<i32>> {
        self.child_event_rx.take()
    }
}
