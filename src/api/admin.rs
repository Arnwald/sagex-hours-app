//! Who is calling, and the instance's users (administrators only).

use crate::api::{ApiError, ApiResult};
use crate::app::{AppState, Caller};
use crate::users::{NewUser, UserInfo};
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

/// The caller and the kind of instance: what the interface adapts to.
pub async fn me(State(state): State<AppState>, caller: Caller) -> Json<Value> {
  Json(json!({
    "user": caller.info(),
    "admin": caller.is_admin(),
    "multi_user": state.is_multi_user(),
    "key_required": state.token_required(),
  }))
}

fn require_admin(caller: &Caller) -> ApiResult<()> {
  if caller.is_admin() {
    Ok(())
  } else {
    Err(ApiError::forbidden(tr!("this action is reserved for administrators")))
  }
}

fn require_multi(state: &AppState) -> ApiResult<()> {
  if state.is_multi_user() {
    Ok(())
  } else {
    Err(ApiError::conflict(tr!("this instance has a single user — enable several users first")))
  }
}

/// Mistakes in a users request are the caller's to fix (a name taken…).
fn user_error(err: anyhow::Error) -> ApiError {
  ApiError::bad_request(format!("{err:#}"))
}

pub async fn list_users(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Vec<UserInfo>>> {
  require_admin(&caller)?;
  require_multi(&state)?;
  Ok(Json(state.users().iter().map(UserInfo::from).collect()))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewUserInput {
  pub name: String,
  #[serde(default)]
  pub id: Option<String>,
  #[serde(default)]
  pub admin: bool,
  /// Also start from the administrator's projects (the activities always are).
  #[serde(default)]
  pub with_projects: bool,
}

/// Creates a user; their key is in the answer, and only there.
pub async fn add_user(State(state): State<AppState>, caller: Caller, Json(input): Json<NewUserInput>) -> ApiResult<Json<Value>> {
  require_admin(&caller)?;
  require_multi(&state)?;
  let template = caller.space.store.catalog()?;
  let (user, key) = state
    .add_user(NewUser {
      name: &input.name,
      id: input.id.as_deref(),
      admin: input.admin,
      template: Some(&template),
      with_projects: input.with_projects,
    })
    .map_err(user_error)?;
  Ok(Json(json!({ "user": UserInfo::from(&user), "key": key })))
}

/// A new key for a user who lost theirs; the old one stops working.
pub async fn renew_key(State(state): State<AppState>, caller: Caller, Path(id): Path<String>) -> ApiResult<Json<Value>> {
  require_admin(&caller)?;
  require_multi(&state)?;
  let (user, key) = state.renew_key(&id).map_err(user_error)?;
  Ok(Json(json!({ "user": UserInfo::from(&user), "key": key })))
}

/// Takes a user off the list; their folder stays on disk.
pub async fn remove_user(State(state): State<AppState>, caller: Caller, Path(id): Path<String>) -> ApiResult<Json<Value>> {
  require_admin(&caller)?;
  require_multi(&state)?;
  if caller.user.as_ref().is_some_and(|u| u.id == id) {
    return Err(ApiError::conflict(tr!("you cannot remove yourself")));
  }
  let user = state.remove_user(&id).map_err(user_error)?;
  Ok(Json(json!({ "removed": user.id })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnableInput {
  pub name: String,
  #[serde(default)]
  pub id: Option<String>,
}

/// Single-user → several users: the current base becomes the caller's, who
/// becomes the first administrator, keeping the current key if there is one.
pub async fn enable_users(State(state): State<AppState>, caller: Caller, Json(input): Json<EnableInput>) -> ApiResult<Json<Value>> {
  require_admin(&caller)?;
  if state.is_multi_user() {
    return Err(ApiError::conflict(tr!("this instance already has several users")));
  }
  let (user, key) = state.enable_users(&input.name, input.id.as_deref()).map_err(user_error)?;
  Ok(Json(json!({ "user": UserInfo::from(&user), "key": key })))
}
