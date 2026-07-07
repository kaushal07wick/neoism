use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use neoism_agent_core::ProviderStreamEvent;
use tokio_stream::StreamExt;

use crate::provider;

pub(crate) enum ProviderEventPoll {
    Event(anyhow::Result<ProviderStreamEvent>),
    End,
    Cancelled,
}

pub(crate) async fn next_provider_stream_event(
    provider_events: &mut provider::ProviderEventStream,
    cancellation: &Arc<AtomicBool>,
) -> ProviderEventPoll {
    if cancellation.load(Ordering::SeqCst) {
        return ProviderEventPoll::Cancelled;
    }
    tokio::select! {
        event = provider_events.next() => match event {
            Some(event) => ProviderEventPoll::Event(event),
            None => ProviderEventPoll::End,
        },
        _ = wait_for_cancellation(cancellation.clone()) => ProviderEventPoll::Cancelled,
    }
}

pub(crate) async fn wait_for_cancellation(cancellation: Arc<AtomicBool>) {
    while !cancellation.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
