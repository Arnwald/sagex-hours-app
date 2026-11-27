//! Serves the interface's files, embedded in the binary.
//!
//! In debug builds, `rust-embed` reads the `web/` folder again on every
//! request: the interface can be changed without recompiling.

use axum::extract::Path;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/"]
struct Assets;

pub fn router() -> Router {
  Router::new()
    .route("/", get(|| async { page("index.html") }))
    .route("/manage", get(|| async { page("manage.html") }))
    .route("/{*path}", get(asset))
}

async fn asset(Path(path): Path<String>) -> Response {
  page(&path)
}

fn page(path: &str) -> Response {
  match Assets::get(path) {
    Some(file) => {
      let mime = mime_guess::from_path(path).first_or_octet_stream();
      ([(header::CONTENT_TYPE, mime.as_ref())], file.data.into_owned()).into_response()
    }
    None => (StatusCode::NOT_FOUND, "not found").into_response(),
  }
}

/// Used by the main router for unknown URLs.
pub async fn not_found(uri: Uri) -> Response {
  (StatusCode::NOT_FOUND, format!("not found: {uri}")).into_response()
}
