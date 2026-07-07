//! A blocking-socket [`SyncPeer`] over TCP.
//!
//! This is the live pipe between two Neoism instances on a network: each
//! side drains its [`SyncDoc`](crate::SyncDoc)'s local updates and
//! [`send`](SyncPeer::send)s them, then [`poll`](SyncPeer::poll)s for the
//! peer's updates and imports them. Framing is a little-endian `u32`
//! length prefix per blob. mDNS discovery (the "AirPods-fast" auto-find)
//! lives in [`crate::discovery`] and just hands us an address to
//! [`connect`](TcpPeer::connect) to — the wire protocol is identical
//! whether we found the peer by hand, by mDNS, or (later) via a cloud
//! relay.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

use crate::peer::{PeerId, SyncPeer};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// One end of a TCP sync connection. Reads are non-blocking so
/// [`poll`](SyncPeer::poll) never stalls the UI; writes briefly block to
/// flush a whole frame.
pub struct TcpPeer {
    stream: TcpStream,
    inbuf: Vec<u8>,
    remote: Option<PeerId>,
    connected: bool,
}

impl TcpPeer {
    /// Wrap an already-accepted stream (server side).
    pub fn new(stream: TcpStream) -> std::io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            inbuf: Vec::new(),
            remote: None,
            connected: true,
        })
    }

    /// Dial a peer (client side) — e.g. an address mDNS resolved.
    pub fn connect(addr: impl ToSocketAddrs) -> std::io::Result<Self> {
        Self::new(TcpStream::connect(addr)?)
    }

    /// Record who's on the other end once a handshake has identified them.
    pub fn set_remote_id(&mut self, id: PeerId) {
        self.remote = Some(id);
    }
}

impl SyncPeer for TcpPeer {
    type Error = NetError;

    fn remote_id(&self) -> Option<PeerId> {
        self.remote
    }

    fn send(&mut self, update: &[u8]) -> Result<(), NetError> {
        let header = (update.len() as u32).to_le_bytes();
        // Flip to blocking for the duration of the write so a frame goes
        // out whole, then restore non-blocking for reads.
        self.stream.set_nonblocking(false)?;
        let result = self
            .stream
            .write_all(&header)
            .and_then(|()| self.stream.write_all(update))
            .and_then(|()| self.stream.flush());
        let restore = self.stream.set_nonblocking(true);
        if let Err(e) = result {
            self.connected = false;
            return Err(e.into());
        }
        restore?;
        Ok(())
    }

    fn poll(&mut self) -> Result<Vec<Vec<u8>>, NetError> {
        let mut tmp = [0u8; 8192];
        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => {
                    self.connected = false;
                    break;
                }
                Ok(n) => self.inbuf.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.connected = false;
                    return Err(e.into());
                }
            }
        }

        let mut out = Vec::new();
        let mut off = 0;
        loop {
            if self.inbuf.len() - off < 4 {
                break;
            }
            let len =
                u32::from_le_bytes(self.inbuf[off..off + 4].try_into().unwrap()) as usize;
            if self.inbuf.len() - off - 4 < len {
                break; // frame not fully arrived yet
            }
            out.push(self.inbuf[off + 4..off + 4 + len].to_vec());
            off += 4 + len;
        }
        if off > 0 {
            self.inbuf.drain(..off);
        }
        Ok(out)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoteDoc;
    use std::net::TcpListener;
    use std::time::Duration;

    fn drain_into(peer: &mut TcpPeer, doc: &NoteDoc) -> usize {
        let mut applied = 0;
        for _ in 0..100 {
            let blobs = peer.poll().unwrap();
            for blob in &blobs {
                doc.sync().import(blob).unwrap();
                applied += 1;
            }
            if applied > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        applied
    }

    #[test]
    fn two_notes_sync_text_and_ink_over_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            TcpPeer::new(stream).unwrap()
        });
        let mut client = TcpPeer::connect(addr).unwrap();
        let mut server = accept.join().unwrap();

        // A authors on the client side and ships a snapshot.
        let a = NoteDoc::with_peer_id(1);
        a.set_markdown("hello from A");
        client.send(&a.sync().snapshot().unwrap()).unwrap();

        // B receives it on the server side.
        let b = NoteDoc::with_peer_id(2);
        assert!(drain_into(&mut server, &b) > 0, "expected a frame");
        assert_eq!(b.markdown(), "hello from A");

        // Now B draws a stroke and ships the delta back; A merges it.
        use crate::{Color, Stroke, StrokePoint};
        b.add_stroke(&Stroke::new(
            7,
            vec![StrokePoint {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
            }],
            1.0,
            Color::BLACK,
        ))
        .unwrap();
        server
            .send(&b.sync().export_from(&a.sync().version()).unwrap())
            .unwrap();
        assert!(drain_into(&mut client, &a) > 0, "expected delta back");
        assert_eq!(a.strokes().len(), 1);
    }

    #[test]
    fn poll_is_nonblocking_when_idle() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            TcpPeer::new(stream).unwrap()
        });
        let _client = TcpPeer::connect(addr).unwrap();
        let mut server = accept.join().unwrap();
        // Nothing sent → poll returns immediately with no frames.
        assert!(server.poll().unwrap().is_empty());
    }
}
