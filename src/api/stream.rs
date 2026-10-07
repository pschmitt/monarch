use std::{convert::Infallible, time::Duration};

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::{Stream, StreamExt};
use serde_json::json;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};

use crate::{
    auth::User,
    state::{SharedState, now},
};

pub async fn stream(
    State(state): State<SharedState>,
    _user: User,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let updates = BroadcastStream::new(state.stream.subscribe())
        // A lagging client misses some updates; it resyncs on its next fetch.
        .filter_map(|m| async move { m.ok() })
        .map(|m| Ok(Event::default().data(m.as_ref())));
    let pings = IntervalStream::new(tokio::time::interval(Duration::from_secs(25)))
        .map(|_| Ok(Event::default().data(json!({"type": "ping", "ts": now()}).to_string())));
    Sse::new(futures::stream::select(updates, pings)).keep_alive(KeepAlive::default())
}
