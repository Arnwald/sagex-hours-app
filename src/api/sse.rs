//! Flux d'évènements : toute modification de la base est poussée aux navigateurs.

use crate::app::AppState;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

pub async fn stream(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
  let changes = BroadcastStream::new(state.store.subscribe()).filter_map(|change| {
    let change = change.ok()?;
    let data = serde_json::to_string(&change).ok()?;
    Some(Ok(Event::default().event("change").data(data)))
  });

  // Le keep-alive maintient la connexion à travers les proxies (TrueNAS, reverse proxy).
  Sse::new(changes).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)).text("ping"))
}
