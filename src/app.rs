//! Shared server state: one base (single-user mode) or one base per user.

use crate::store::Store;
use crate::users::{self, User, UserInfo};
use anyhow::{Context, Result};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock};

/// One person's base, with what serialises its writes and filters the echo of
/// its own writes out of the folder watcher.
#[derive(Clone)]
pub struct Space {
  pub store: Store,
  /// Serialises the writes of this base: a read-modify-write is never
  /// interleaved with another.
  pub write_lock: Arc<tokio::sync::Mutex<()>>,
  /// When the server last wrote to this base, so that the folder watcher does
  /// not broadcast our own changes a second time.
  last_self_write: Arc<AtomicI64>,
}

impl Space {
  pub fn open(root: impl AsRef<Path>) -> Result<Self> {
    Ok(Space {
      store: Store::open(root)?,
      write_lock: Arc::new(tokio::sync::Mutex::new(())),
      last_self_write: Arc::new(AtomicI64::new(0)),
    })
  }

  pub fn mark_self_write(&self) {
    self.last_self_write.store(now_millis(), Ordering::Relaxed);
  }

  /// True if the server wrote to this base less than `window_ms` ago.
  pub fn wrote_recently(&self, window_ms: i64) -> bool {
    now_millis() - self.last_self_write.load(Ordering::Relaxed) < window_ms
  }
}

/// Who is calling, as resolved from their key: their base, and who they are
/// (`None` in single-user mode, where the one person is the owner).
#[derive(Clone)]
pub struct Caller {
  pub space: Space,
  pub user: Option<User>,
  /// The key presented, to hand it on (the Claude Desktop extension embeds it).
  pub key: Option<String>,
}

impl Caller {
  pub fn is_admin(&self) -> bool {
    self.user.as_ref().is_none_or(|u| u.admin)
  }

  pub fn info(&self) -> Option<UserInfo> {
    self.user.as_ref().map(UserInfo::from)
  }
}

enum Mode {
  Single(Space),
  Multi { users: Vec<User>, spaces: HashMap<String, Space> },
}

#[derive(Clone)]
pub struct AppState {
  inner: Arc<Inner>,
}

struct Inner {
  root: PathBuf,
  /// `SAGEX_TOKEN`: the access token of single-user mode, and the first
  /// administrator's key when several users are enabled.
  token: Option<String>,
  mode: RwLock<Mode>,
  /// Serialises the changes to `users.json`.
  admin_lock: std::sync::Mutex<()>,
}

impl AppState {
  /// Opens the data folder: several users if `users.json` is there, else one base.
  pub fn open(root: impl AsRef<Path>, token: Option<String>) -> Result<Self> {
    let root = root.as_ref();
    std::fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    // Absolute and canonical, like the paths the folder watcher reports.
    let root = root.canonicalize().with_context(|| format!("resolving {}", root.display()))?;
    let mode = match users::load(&root)? {
      Some(file) => multi(&root, file.users, &HashMap::new())?,
      None => Mode::Single(Space::open(&root)?),
    };
    Ok(AppState { inner: Arc::new(Inner { root, token, mode: RwLock::new(mode), admin_lock: Default::default() }) })
  }

  pub fn root(&self) -> &Path {
    &self.inner.root
  }

  pub fn is_multi_user(&self) -> bool {
    matches!(*self.read(), Mode::Multi { .. })
  }

  pub fn token_required(&self) -> bool {
    match &*self.read() {
      Mode::Single(_) => self.inner.token.is_some(),
      Mode::Multi { .. } => true,
    }
  }

  pub fn users(&self) -> Vec<User> {
    match &*self.read() {
      Mode::Single(_) => Vec::new(),
      Mode::Multi { users, .. } => users.clone(),
    }
  }

  /// The caller a key opens, if any.
  pub fn resolve(&self, key: Option<&str>) -> Option<Caller> {
    let key = key.map(str::trim).filter(|k| !k.is_empty());
    match &*self.read() {
      Mode::Single(space) => match (&self.inner.token, key) {
        (None, _) => Some(Caller { space: space.clone(), user: None, key: None }),
        (Some(expected), Some(given)) if given == expected => {
          Some(Caller { space: space.clone(), user: None, key: Some(given.to_string()) })
        }
        _ => None,
      },
      Mode::Multi { users, spaces } => {
        let hash = users::hash_key(key?);
        let user = users.iter().find(|u| u.key_sha256 == hash)?;
        let space = spaces.get(&user.id)?;
        Some(Caller { space: space.clone(), user: Some(user.clone()), key: key.map(str::to_string) })
      }
    }
  }

  /// The base a changed file belongs to, for the folder watcher.
  pub fn space_of(&self, path: &Path) -> Option<Space> {
    match &*self.read() {
      Mode::Single(space) => Some(space.clone()),
      Mode::Multi { spaces, .. } => {
        let relative = path.strip_prefix(self.inner.root.join("users")).ok()?;
        let id = relative.components().next()?.as_os_str().to_string_lossy().to_string();
        spaces.get(&id).cloned()
      }
    }
  }

  /// Re-reads `users.json` (changed by hand, by the command line or by the
  /// API). Bases already open keep their state, so live pages stay connected.
  pub fn reload_users(&self) -> Result<()> {
    let Some(file) = users::load(&self.inner.root)? else {
      return Ok(());
    };
    let current = match &*self.read() {
      Mode::Single(_) => HashMap::new(),
      Mode::Multi { spaces, .. } => spaces.clone(),
    };
    let next = multi(&self.inner.root, file.users, &current)?;
    *self.inner.mode.write().expect("state lock poisoned") = next;
    Ok(())
  }

  // -- managing users (administrators) ---------------------------------------

  pub fn enable_users(&self, name: &str, id: Option<&str>) -> Result<(User, String)> {
    let _guard = self.inner.admin_lock.lock().expect("admin lock poisoned");
    let created = users::enable(&self.inner.root, name, id, self.inner.token.as_deref())?;
    self.reload_users()?;
    Ok(created)
  }

  pub fn add_user(&self, options: users::NewUser) -> Result<(User, String)> {
    let _guard = self.inner.admin_lock.lock().expect("admin lock poisoned");
    let created = users::add(&self.inner.root, options)?;
    self.reload_users()?;
    Ok(created)
  }

  pub fn renew_key(&self, id: &str) -> Result<(User, String)> {
    let _guard = self.inner.admin_lock.lock().expect("admin lock poisoned");
    let renewed = users::renew_key(&self.inner.root, id)?;
    self.reload_users()?;
    Ok(renewed)
  }

  pub fn remove_user(&self, id: &str) -> Result<User> {
    let _guard = self.inner.admin_lock.lock().expect("admin lock poisoned");
    let removed = users::remove(&self.inner.root, id)?;
    self.reload_users()?;
    Ok(removed)
  }

  fn read(&self) -> std::sync::RwLockReadGuard<'_, Mode> {
    self.inner.mode.read().expect("state lock poisoned")
  }
}

/// Opens the base of every user, reusing those already open.
fn multi(root: &Path, users: Vec<User>, open: &HashMap<String, Space>) -> Result<Mode> {
  let mut spaces = HashMap::new();
  for user in &users {
    let space = match open.get(&user.id) {
      Some(space) => space.clone(),
      None => Space::open(users::user_dir(root, &user.id))?,
    };
    spaces.insert(user.id.clone(), space);
  }
  Ok(Mode::Multi { users, spaces })
}

fn now_millis() -> i64 {
  chrono::Utc::now().timestamp_millis()
}

/// Handlers take the caller (or just their base) as an argument; the access
/// middleware has put it in the request.
impl<S: Send + Sync> FromRequestParts<S> for Caller {
  type Rejection = StatusCode;

  async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
    parts.extensions.get::<Caller>().cloned().ok_or(StatusCode::UNAUTHORIZED)
  }
}

impl<S: Send + Sync> FromRequestParts<S> for Space {
  type Rejection = StatusCode;

  async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
    parts.extensions.get::<Caller>().map(|c| c.space.clone()).ok_or(StatusCode::UNAUTHORIZED)
  }
}
