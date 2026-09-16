//! Service des fichiers de l'interface, embarqués dans le binaire.
//!
//! En debug, `rust-embed` relit le dossier `web/` à chaque requête : on peut
//! modifier l'interface sans recompiler.

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
    None => (StatusCode::NOT_FOUND, "introuvable").into_response(),
  }
}

/// Utilisé par le routeur principal pour les URL inconnues.
pub async fn not_found(uri: Uri) -> Response {
  (StatusCode::NOT_FOUND, format!("introuvable : {uri}")).into_response()
}
