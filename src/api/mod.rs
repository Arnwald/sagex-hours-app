//! HTTP router, access control and the common error type.

pub mod admin;
pub mod backup;
pub mod catalog;
pub mod claude;
pub mod entries;
pub mod export;
pub mod sse;

use crate::app::AppState;
use crate::i18n::{self, Lang};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde_json::json;

/// API error rendered as JSON: `{"error": "..."}`.
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
  pub fn forbidden(message: impl Into<String>) -> Self {
    ApiError { status: StatusCode::FORBIDDEN, message: message.into() }
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    (self.status, Json(json!({ "error": self.message }))).into_response()
  }
}

/// Any internal error becomes a readable 500.
impl From<anyhow::Error> for ApiError {
  fn from(err: anyhow::Error) -> Self {
    tracing::error!("internal error: {err:#}");
    ApiError { status: StatusCode::INTERNAL_SERVER_ERROR, message: format!("{err:#}") }
  }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
  let routes = Router::new()
    .route("/state", get(entries::state))
    .route("/me", get(admin::me))
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
    .route("/claude/sagex-hours.mcpb", get(claude::desktop_extension))
    .route("/admin/users", get(admin::list_users).post(admin::add_user))
    .route("/admin/users/{id}", delete(admin::remove_user))
    .route("/admin/users/{id}/key", post(admin::renew_key))
    .route("/admin/enable-users", post(admin::enable_users))
    .with_state(state.clone());

  Router::new().nest("/api", routes).layer(middleware::from_fn_with_state(state, access))
}

/// Every API call: the language of its messages, then the caller its key
/// opens — refused without one when the instance asks for a key.
async fn access(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
  let lang = request_lang(request.uri(), request.headers());
  i18n::scope(lang, async move {
    let key = presented_key(request.uri(), request.headers());
    match state.resolve(key.as_deref()) {
      Some(caller) => {
        request.extensions_mut().insert(caller);
        next.run(request).await
      }
      None => ApiError { status: StatusCode::UNAUTHORIZED, message: tr!("invalid or missing access key") }.into_response(),
    }
  })
  .await
}

/// `?lang=`, then `Accept-Language`; English by default (curl, Claude).
pub fn request_lang(uri: &Uri, headers: &HeaderMap) -> Lang {
  query_param(uri, "lang")
    .and_then(|code| Lang::from_code(&code))
    .or_else(|| headers.get("accept-language").and_then(|v| v.to_str().ok()).and_then(Lang::from_accept_language))
    .unwrap_or(Lang::En)
}

/// The key of a request: `X-Auth-Token`, `Authorization: Bearer …` or `?token=`.
pub fn presented_key(uri: &Uri, headers: &HeaderMap) -> Option<String> {
  headers
    .get("x-auth-token")
    .and_then(|v| v.to_str().ok())
    .map(str::to_string)
    .or_else(|| {
      headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(str::to_string)
    })
    .or_else(|| query_param(uri, "token"))
    .map(|key| key.split_whitespace().collect::<String>())
    .filter(|key| !key.is_empty())
}

fn query_param(uri: &Uri, name: &str) -> Option<String> {
  uri.query()?.split('&').find_map(|pair| {
    let (key, value) = pair.split_once('=')?;
    (key == name).then(|| percent_decode(value))
  })
}

/// Enough of URL decoding for keys and language codes.
fn percent_decode(value: &str) -> String {
  let bytes = value.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'%' if i + 2 < bytes.len() + 0 && i + 2 <= bytes.len() - 1 => {
        match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
          Ok(byte) => {
            out.push(byte);
            i += 3;
          }
          Err(_) => {
            out.push(b'%');
            i += 1;
          }
        }
      }
      b'+' => {
        out.push(b' ');
        i += 1;
      }
      byte => {
        out.push(byte);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keys_and_languages_are_read_from_headers_or_query() {
    let uri: Uri = "/api/backup?lang=fr&token=ab%20cd".parse().unwrap();
    let headers = HeaderMap::new();
    assert_eq!(request_lang(&uri, &headers), Lang::Fr);
    assert_eq!(presented_key(&uri, &headers).as_deref(), Some("abcd"), "spaces in a pasted key are dropped");

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer k1".parse().unwrap());
    headers.insert("accept-language", "fr-CH,fr;q=0.9".parse().unwrap());
    let plain: Uri = "/api/state".parse().unwrap();
    assert_eq!(presented_key(&plain, &headers).as_deref(), Some("k1"));
    assert_eq!(request_lang(&plain, &headers), Lang::Fr);
    assert_eq!(request_lang(&plain, &HeaderMap::new()), Lang::En);
    assert_eq!(percent_decode("a%2Fb%zz%2"), "a/b%zz%2");
  }
}
