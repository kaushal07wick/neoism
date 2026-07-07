use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use neoism_agent_core::{event_type, EventPayload};
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;

use crate::state::AppState;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EventStreamQuery {
    since: Option<i64>,
    limit: Option<usize>,
    #[serde(rename = "sessionID")]
    session_id: Option<String>,
}

pub(crate) async fn event_stream(
    State(state): State<AppState>,
    Query(query): Query<EventStreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    stream_events(state, query)
}

pub(crate) async fn global_event(
    State(state): State<AppState>,
    Query(query): Query<EventStreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    stream_events(state, query)
}

fn stream_events(
    state: AppState,
    query: EventStreamQuery,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut receiver = BroadcastStream::new(state.subscribe());
    let stream = async_stream::stream! {
        yield Ok(sse_json(EventPayload::new(event_type::SERVER_CONNECTED, json!({}))));
        let replay = state
            .inner
            .store
            .list_events_after(
                query.since.unwrap_or(0),
                query.limit.unwrap_or(1_000),
                query.session_id.as_deref(),
            )
            .await
            .unwrap_or_default();
        for event in replay {
            yield Ok(sse_json(event.payload));
        }
        loop {
            match tokio_stream::StreamExt::next(&mut receiver).await {
                Some(Ok(event)) => yield Ok(sse_json(event)),
                Some(Err(_)) => continue,
                None => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10)).event(
        sse_json(EventPayload::new(event_type::SERVER_HEARTBEAT, json!({}))),
    ))
}

fn sse_json(payload: EventPayload) -> Event {
    Event::default()
        .id(payload.id.to_string())
        .event(payload.kind.clone())
        .data(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()))
}
