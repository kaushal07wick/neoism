//! The reMarkable bridge wire protocol, shared by the on-device agent and
//! Neoism so the two can't drift.
//!
//! The agent watches xochitl's `.rm` files and, whenever a page changes,
//! sends the full set of strokes for that page. Neoism merges them into
//! the matching note's CRDT ink layer (by stable stroke id, so resends of
//! an unchanged page are no-ops). Frames are a little-endian `u32` length
//! prefix + JSON body — same framing convention as [`crate::net`].

use std::io::{ErrorKind, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};

use serde::{Deserialize, Serialize};

use crate::stroke::Stroke;

/// A message on the bridge channel (device agent ⇄ Neoism).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum BridgeMsg {
    /// Agent's greeting on connect — lets Neoism pick the right `.rm`
    /// decoder and show the device.
    Hello { device: String, firmware: String },
    /// The full current ink for one notebook page. Full-page (not diff) so
    /// it's self-healing: a dropped update is corrected by the next one.
    PageInk {
        page_id: String,
        strokes: Vec<Stroke>,
    },
}

impl BridgeMsg {
    /// Serialize to a length-prefixed frame ready to write to a socket.
    pub fn encode_frame(&self) -> Vec<u8> {
        let body = serde_json::to_vec(self).unwrap_or_default();
        let mut frame = (body.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&body);
        frame
    }

    /// Pull every complete message out of a rolling receive buffer,
    /// leaving any partial trailing frame in place for the next read.
    pub fn drain(buf: &mut Vec<u8>) -> Vec<BridgeMsg> {
        let mut out = Vec::new();
        let mut off = 0;
        loop {
            if buf.len() - off < 4 {
                break;
            }
            let len = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
            if buf.len() - off - 4 < len {
                break;
            }
            if let Ok(msg) =
                serde_json::from_slice::<BridgeMsg>(&buf[off + 4..off + 4 + len])
            {
                out.push(msg);
            }
            off += 4 + len;
        }
        if off > 0 {
            buf.drain(..off);
        }
        out
    }
}

/// Neoism's end of the bridge: listens for the on-device agent and hands
/// back decoded [`BridgeMsg`]s. Non-blocking, so the UI loop can `poll`
/// each frame. Feed the messages to [`NoteDoc::apply_bridge`] to land ink
/// in the right note.
///
/// [`NoteDoc::apply_bridge`]: crate::NoteDoc::apply_bridge
pub struct BridgeServer {
    listener: TcpListener,
    client: Option<TcpStream>,
    inbuf: Vec<u8>,
}

impl BridgeServer {
    /// Bind a listening socket (e.g. `"0.0.0.0:0"` to pick a free port).
    pub fn bind(addr: impl ToSocketAddrs) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            client: None,
            inbuf: Vec::new(),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// Accept a waiting agent (if we don't have one) and return any
    /// complete messages received since the last call. Never blocks.
    pub fn poll(&mut self) -> Vec<BridgeMsg> {
        if self.client.is_none() {
            if let Ok((stream, _)) = self.listener.accept() {
                let _ = stream.set_nonblocking(true);
                self.client = Some(stream);
                self.inbuf.clear();
            }
        }
        if let Some(stream) = self.client.as_mut() {
            let mut tmp = [0u8; 8192];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => {
                        self.client = None;
                        break;
                    }
                    Ok(n) => self.inbuf.extend_from_slice(&tmp[..n]),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        self.client = None;
                        break;
                    }
                }
            }
        }
        BridgeMsg::drain(&mut self.inbuf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, StrokePoint};

    #[test]
    fn frames_roundtrip_through_a_split_buffer() {
        let hello = BridgeMsg::Hello {
            device: "reMarkable 2".into(),
            firmware: "3.11".into(),
        };
        let ink = BridgeMsg::PageInk {
            page_id: "page-uuid".into(),
            strokes: vec![Stroke::new(
                9,
                vec![StrokePoint {
                    x: 1.0,
                    y: 2.0,
                    pressure: 1.0,
                }],
                2.0,
                Color::BLACK,
            )],
        };

        let mut wire = hello.encode_frame();
        wire.extend(ink.encode_frame());

        // Feed it in two awkward chunks to prove partial-frame handling.
        let split = wire.len() / 3;
        let mut buf = Vec::new();
        buf.extend_from_slice(&wire[..split]);
        let first = BridgeMsg::drain(&mut buf);
        buf.extend_from_slice(&wire[split..]);
        let rest = BridgeMsg::drain(&mut buf);

        let mut all = first;
        all.extend(rest);
        assert_eq!(all, vec![hello, ink]);
        assert!(buf.is_empty(), "no trailing bytes left over");
    }

    #[test]
    fn server_receives_pageink_into_a_note() {
        use crate::NoteDoc;
        use std::io::Write;
        use std::time::Duration;

        let mut server = BridgeServer::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();

        // The "agent" connects and streams a greeting + one page of ink.
        let mut agent = TcpStream::connect(addr).unwrap();
        let hello = BridgeMsg::Hello {
            device: "reMarkable".into(),
            firmware: "3.27".into(),
        };
        let ink = BridgeMsg::PageInk {
            page_id: "doc/p1".into(),
            strokes: vec![
                Stroke::new(
                    1,
                    vec![StrokePoint {
                        x: 1.0,
                        y: 1.0,
                        pressure: 1.0,
                    }],
                    2.0,
                    Color::BLACK,
                ),
                Stroke::new(
                    2,
                    vec![StrokePoint {
                        x: 2.0,
                        y: 2.0,
                        pressure: 1.0,
                    }],
                    2.0,
                    Color::BLACK,
                ),
            ],
        };
        agent.write_all(&hello.encode_frame()).unwrap();
        agent.write_all(&ink.encode_frame()).unwrap();
        agent.flush().unwrap();

        let note = NoteDoc::new();
        let mut received = 0;
        for _ in 0..100 {
            for msg in server.poll() {
                note.apply_bridge(&msg).unwrap();
                received += 1;
            }
            if note.page_strokes("doc/p1").len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(received >= 1, "server saw no messages");
        assert_eq!(note.page_strokes("doc/p1").len(), 2);
    }
}
