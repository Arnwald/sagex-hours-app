//! Routeur HTTP et type d'erreur commun.

pub mod backup;
pub mod catalog;
pub mod entries;
pub mod export;
pub mod sse;

use crate::app::AppState;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde_json::json;

/// Erreur d'API rendue en JSON : `{"error": "..."}`.
#[derive(Debug)]
pub struct ApiError {
  pub status: StatusCode,
  pub message: String,
}

impl ApiError {
  pub fn bad_request(message: impl Into<String>) -> Self {
    ApiError { status: StatusCode::BAD_REQUEST, message: message.into() }
  }
  pub fn not_found(message: impl Into<String>) -> Self {
    ApiError { status: StatusCode::NOT_FOUND, message: message.into() }
  }
  pub fn conflict(message: impl Into<String>) -> Self {
    ApiError { status: StatusCode::CONFLICT, message: message.into() }
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    (self.status, Json(json!({ "error": self.message }))).into_response()
  }
}

/// Toute erreur interne devient un 500 lisible.
impl From<anyhow::Error> for ApiError {
  fn from(err: anyhow::Error) -> Self {
    tracing::error!("erreur interne : {err:#}");
    ApiError { status: StatusCode::INTERNAL_SERVER_ERROR, message: format!("{err:#}") }
  }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
  let routes = Router::new()
    .route("/state", get(entries::state))
    .route("/settings", get(entries::get_settings).patch(entries::patch_settings))
    .route("/entries", get(entries::list).post(entries::create))
    .route("/entries/copy", post(entries::copy))
    .route("/entries/clear", post(entries::clear))
    .route("/entries/{id}", patch(entries::update).delete(entries::remove))
    .route("/timer", get(entries::get_timer))
    .route("/timer/start", post(entries::start_timer))
    .route("/timer/stop", post(entries::stop_timer))
    .route("/catalog", get(catalog::get_catalog))
    .route("/catalog/{kind}", post(catalog::create_item))
    .route("/catalog/{kind}/{id}", patch(catalog::update_item))
    .route("/catalog/{kind}/{id}", delete(catalog::delete_item))
    .route("/month/{month}/report", get(export::month_report))
    .route("/export/{month}", get(export::download))
    .route("/backup", get(backup::download))
    .route("/restore", post(backup::restore))
    .route("/import/toggl", post(entries::import_toggl))
    .route("/stream", get(sse::stream))
    .with_state(state.clone());

  Router::new().nest("/api", routes).layer(middleware::from_fn_with_state(state, auth))
}

/// Jeton facultatif : en-tête `Authorization: Bearer …`, `X-Auth-Token` ou `?token=`.
async fn auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
  let Some(expected) = state.auth_token.as_deref() else {
    return next.run(request).await;
  };

  let headers = request.headers();
  let from_header = headers
    .get("x-auth-token")
    .and_then(|v| v.to_str().ok())
    .map(str::to_string)
    .or_else(|| {
      headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
    })
    .or_else(|| {
      request.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| pair.strip_prefix("token=").map(|t| t.to_string()))
      })
    });

  match from_header {
    Some(token) if token == expected => next.run(request).await,
    _ => (StatusCode::UNAUTHORIZED, Json(json!({ "error": "jeton d'accès invalide ou absent" }))).into_response(),
  }
}
