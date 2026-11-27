//! Event stream: every change to a base is pushed to the browsers showing it.

use crate::app::Space;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

pub async fn stream(space: Space) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
  let changes = BroadcastStream::new(space.store.subscribe()).filter_map(|change| {
    let change = change.ok()?;
    let data = serde_json::to_string(&change).ok()?;
    Some(Ok(Event::default().event("change").data(data)))
  });

  // The keep-alive holds the connection open through proxies (reverse proxies, VPNs).
  Sse::new(changes).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)).text("ping"))
}
