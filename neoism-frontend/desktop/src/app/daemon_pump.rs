use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use crate::daemon_client::{DaemonClient, DaemonClientHandle, DaemonServerMessage};
use crate::event::{EventProxy, RioEvent, RioEventType};
use neoism_protocol::workspace::WorkspaceClientMessage;

pub struct DesktopDaemonConnection {
    _runtime: tokio::runtime::Runtime,
    runtime_handle: tokio::runtime::Handle,
    handle: DaemonClientHandle,
    inbound: Arc<Mutex<VecDeque<DaemonServerMessage>>>,
    inbound_wake_pending: Arc<AtomicBool>,
    /// The endpoint string this connection was dialled against (the
    /// `unix://…` / `ws://…` URL, including any `?token=` auth carried in
    /// the URL). Retained so the "follow the workspace to its new home"
    /// logic in `app::mod` can (a) compare the live endpoint against a
    /// candidate host `daemon_url` to guard against reconnecting to the
    /// URL we're already on, and (b) preserve the auth/token shape when
    /// rebuilding the connection against a new host.
    endpoint: String,
}

impl DesktopDaemonConnection {
    pub fn connect(
        endpoint: &str,
        event_proxy: EventProxy,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .thread_name("neoism-desktop-daemon-client")
            .enable_all()
            .build()?;
        let client = runtime.block_on(DaemonClient::connect(endpoint))?;
        let (handle, mut rx, _status_rx) = client.into_channels();
        let inbound = Arc::new(Mutex::new(VecDeque::new()));
        let inbound_task = Arc::clone(&inbound);
        let inbound_wake_pending = Arc::new(AtomicBool::new(false));
        let inbound_wake_pending_task = Arc::clone(&inbound_wake_pending);
        let runtime_handle = runtime.handle().clone();

        runtime_handle.spawn(async move {
            while let Some(message) = rx.recv().await {
                let should_wake = match inbound_task.lock() {
                    Ok(mut queue) => {
                        queue.push_back(message);
                        !inbound_wake_pending_task.swap(true, Ordering::AcqRel)
                    }
                    Err(error) => {
                        tracing::warn!(
                            target: "neoism::desktop_daemon",
                            %error,
                            "daemon inbound queue poisoned"
                        );
                        break;
                    }
                };
                if should_wake {
                    event_proxy.send_event(RioEventType::Rio(RioEvent::Render), unsafe {
                        neoism_window::window::WindowId::dummy()
                    });
                }
            }
        });

        Ok(Self {
            _runtime: runtime,
            runtime_handle,
            handle,
            inbound,
            inbound_wake_pending,
            endpoint: endpoint.to_string(),
        })
    }

    /// The endpoint URL this connection is dialled against. Used by the
    /// re-home follow logic to avoid re-dialling the daemon we're already
    /// connected to (loop guard) and to know which auth-bearing URL to
    /// preserve when no fresh token is supplied.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn handle(&self) -> DaemonClientHandle {
        self.handle.clone()
    }

    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime_handle.clone()
    }

    pub fn send(&self, message: WorkspaceClientMessage) {
        let handle = self.handle.clone();
        self.runtime_handle.spawn(async move {
            if let Err(error) = handle.send(message).await {
                tracing::warn!(
                    target: "neoism::desktop_daemon",
                    %error,
                    "daemon request failed"
                );
            }
        });
    }

    /// Wave 7A: fire-and-forget CRDT/presence envelope (cursor
    /// publishes are ephemeral — a failed send is just skipped, the
    /// next coalesced publish supersedes it).
    #[allow(dead_code)]
    pub fn send_crdt(&self, message: neoism_protocol::crdt::CrdtClientMessage) {
        let handle = self.handle.clone();
        self.runtime_handle.spawn(async move {
            if let Err(error) = handle.send_crdt(message).await {
                tracing::warn!(
                    target: "neoism::desktop_daemon",
                    %error,
                    "daemon crdt/presence send failed"
                );
            }
        });
    }

    /// Wave 7B: ship a batch of CRDT document-plane envelopes (markdown
    /// pane open/local-edit traffic). One task per batch keeps the
    /// in-batch order (OpenBuffer before any update for the same doc).
    pub fn send_crdt_batch(
        &self,
        messages: Vec<neoism_protocol::crdt::CrdtClientMessage>,
    ) {
        if messages.is_empty() {
            return;
        }
        let handle = self.handle.clone();
        self.runtime_handle.spawn(async move {
            for message in messages {
                if let Err(error) = handle.send_crdt(message).await {
                    tracing::warn!(
                        target: "neoism::desktop_daemon",
                        %error,
                        "daemon crdt request failed"
                    );
                }
            }
        });
    }

    pub fn drain_messages(&self) -> Vec<DaemonServerMessage> {
        match self.inbound.lock() {
            Ok(mut queue) => {
                let messages = queue.drain(..).collect();
                self.inbound_wake_pending.store(false, Ordering::Release);
                messages
            }
            Err(error) => {
                tracing::warn!(
                    target: "neoism::desktop_daemon",
                    %error,
                    "daemon inbound queue poisoned"
                );
                Vec::new()
            }
        }
    }
}
