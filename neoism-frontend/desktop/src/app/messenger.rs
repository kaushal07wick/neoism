use crate::event::Msg;
use neoism_backend::event::WindowId;
use neoism_ui::lifecycle_policy::{bytes_hex_for_log, bytes_text_for_log};
use std::borrow::Cow;
use teletypewriter::WinsizeBuilder;

pub struct Messenger {
    pub channel: corcovado::channel::Sender<Msg>,
}

impl Messenger {
    pub fn new(channel: corcovado::channel::Sender<Msg>) -> Messenger {
        Messenger { channel }
    }

    #[inline]
    pub fn send_bytes(&mut self, string: Vec<u8>) {
        self.send_write(string);
    }

    #[inline]
    pub fn send_write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        let bytes = bytes.into();
        tracing::trace!(
            target: "neoism::messenger",
            byte_len = bytes.len(),
            bytes_hex = %bytes_hex_for_log(bytes.as_ref()),
            bytes_text = %bytes_text_for_log(bytes.as_ref()),
            "send_write called"
        );

        // terminal hangs if we send 0 bytes through.
        if bytes.is_empty() {
            tracing::trace!(target: "neoism::messenger", "send_write dropped empty payload");
            return;
        }

        match self.channel.send(Msg::Input(bytes)) {
            Ok(()) => {
                tracing::trace!(target: "neoism::messenger", "send_write queued input")
            }
            Err(err) => tracing::warn!(
                target: "neoism::messenger",
                "send_write failed to queue input: {err:?}"
            ),
        }
    }

    #[inline]
    pub fn send_resize(&self, new_size: WinsizeBuilder) -> Result<&str, String> {
        match self.channel.send(Msg::Resize(new_size)) {
            Ok(..) => Ok("Resized"),
            Err(..) => Err("Error sending message".to_string()),
        }
    }

    /// Re-home this session's parser driver onto `window_id`. Used when
    /// a workspace is detached/moved into another OS window so the live
    /// PTY keeps emitting events tagged with its new host window. A
    /// no-op for backends without a live IO thread (e.g. editor panes,
    /// whose control channel has no consumer).
    #[inline]
    pub fn send_rebind_window(&self, window_id: WindowId) {
        if let Err(err) = self.channel.send(Msg::RebindWindow(window_id)) {
            tracing::warn!(
                target: "neoism::messenger",
                "send_rebind_window failed to queue rebind: {err:?}"
            );
        }
    }
}
