//! Several people on one instance.
//!
//! Without `users.json`, the data folder is one person's base, guarded by the
//! optional `SAGEX_TOKEN` (single-user mode). With it, each person listed has a
//! base of the same layout in `users/<id>/` and a personal key; the file keeps
//! only the key's SHA-256, so a copy of it opens nothing.

use crate::model::Catalog;
use crate::store::{read_json, write_json, Store};
use anyhow::{bail, Context, Result};
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub admin: bool,
  /// SHA-256 of the personal key, in hex.
  pub key_sha256: String,
  pub created_at: NaiveDateTime,
}

/// What the interface may see of a user (never the key's hash).
#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
  pub id: String,
  pub name: String,
  pub admin: bool,
  pub created_at: NaiveDateTime,
}

impl From<&User> for UserInfo {
  fn from(user: &User) -> Self {
    UserInfo { id: user.id.clone(), name: user.name.clone(), admin: user.admin, created_at: user.created_at }
  }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsersFile {
  #[serde(default)]
  pub users: Vec<User>,
}

pub fn users_path(root: &Path) -> PathBuf {
  root.join("users.json")
}

pub fn user_dir(root: &Path, id: &str) -> PathBuf {
  root.join("users").join(id)
}

/// `None` in single-user mode.
pub fn load(root: &Path) -> Result<Option<UsersFile>> {
  read_json(&users_path(root))
}

pub fn save(root: &Path, file: &UsersFile) -> Result<()> {
  write_json(&users_path(root), file)
}

/// A new personal key: 24 random bytes in hex (48 characters).
pub fn new_key() -> String {
  let mut bytes = [0u8; 24];
  getrandom::fill(&mut bytes).expect("the system's random source is unavailable");
  hex(&bytes)
}

pub fn hash_key(key: &str) -> String {
  hex(&Sha256::digest(key.trim().as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What a single-user base holds, moved as it is into its owner's folder.
const BASE_ITEMS: [&str; 5] = ["settings.json", "catalog.json", "timer.json", "entries", "backups"];

/// A free identifier for `wanted` (an explicit id, else the name): a slug not
/// taken by a user nor by a folder left in `users/`.
fn free_id(root: &Path, existing: &[User], wanted: &str) -> Result<String> {
  let base = crate::slug::slugify(wanted);
  if base.is_empty() {
    bail!(tr!("a name is needed"));
  }
  let mut taken: HashSet<String> = existing.iter().map(|u| u.id.clone()).collect();
  if let Ok(dir) = std::fs::read_dir(root.join("users")) {
    taken.extend(dir.flatten().map(|e| e.file_name().to_string_lossy().to_string()));
  }
  Ok(crate::slug::unique(&base, &mut taken, None))
}

/// Turns a single-user data folder into a multi-user one: its first user, an
/// administrator, owns the existing base. `key` keeps an existing key (the
/// current `SAGEX_TOKEN`), so the browsers and Claude set up with it go on
/// working; without one, a new key is made. Returns the user and the key.
pub fn enable(root: &Path, name: &str, id: Option<&str>, key: Option<&str>) -> Result<(User, String)> {
  if users_path(root).exists() {
    bail!(tr!("this instance already has several users"));
  }
  let name = name.trim();
  if name.is_empty() {
    bail!(tr!("a name is needed"));
  }
  let id = free_id(root, &[], id.unwrap_or(name))?;
  let dir = user_dir(root, &id);
  std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
  for item in BASE_ITEMS {
    let from = root.join(item);
    if from.exists() {
      let to = dir.join(item);
      std::fs::rename(&from, &to).with_context(|| format!("moving {} to {}", from.display(), to.display()))?;
    }
  }
  // Initialises whatever the base lacked (a fresh instance has nothing yet).
  Store::open(&dir)?;

  let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).unwrap_or_else(new_key);
  let user = User { id, name: name.to_string(), admin: true, key_sha256: hash_key(&key), created_at: now() };
  save(root, &UsersFile { users: vec![user.clone()] })?;
  Ok((user, key))
}

/// Options of a new user.
#[derive(Debug, Default)]
pub struct NewUser<'a> {
  pub name: &'a str,
  pub id: Option<&'a str>,
  pub admin: bool,
  /// Catalogue to start from: its activities (SageX's list is the same for
  /// everyone), and its projects if `with_projects`. Tasks stay personal.
  pub template: Option<&'a Catalog>,
  pub with_projects: bool,
}

/// Adds a user with a base of their own. Returns the user and their key.
pub fn add(root: &Path, options: NewUser) -> Result<(User, String)> {
  let mut file = load(root)?.with_context(|| tr!("this instance has a single user — enable several users first"))?;
  let name = options.name.trim();
  if name.is_empty() {
    bail!(tr!("a name is needed"));
  }
  if let Some(id) = options.id {
    if file.users.iter().any(|u| u.id == id) {
      bail!(tr!("the identifier “{id}” is already taken", id = id));
    }
  }
  let id = free_id(root, &file.users, options.id.unwrap_or(name))?;
  let store = Store::open(user_dir(root, &id))?;
  if let Some(template) = options.template {
    let catalog = Catalog {
      activities: template.activities.iter().filter(|a| !a.archived).cloned().collect(),
      projects: if options.with_projects { template.projects.iter().filter(|p| !p.archived).cloned().collect() } else { Vec::new() },
      presets: Vec::new(),
    };
    store.save_catalog(&catalog)?;
  }

  let key = new_key();
  let user = User { id, name: name.to_string(), admin: options.admin, key_sha256: hash_key(&key), created_at: now() };
  file.users.push(user.clone());
  save(root, &file)?;
  Ok((user, key))
}

/// Gives `id` a new key; the old one stops working at once.
pub fn renew_key(root: &Path, id: &str) -> Result<(User, String)> {
  let mut file = load(root)?.with_context(|| tr!("this instance has a single user — enable several users first"))?;
  let key = new_key();
  let user = file.users.iter_mut().find(|u| u.id == id).with_context(|| tr!("no user “{id}”", id = id))?;
  user.key_sha256 = hash_key(&key);
  let user = user.clone();
  save(root, &file)?;
  Ok((user, key))
}

/// Takes `id` off the list: their key stops working, their folder stays.
pub fn remove(root: &Path, id: &str) -> Result<User> {
  let mut file = load(root)?.with_context(|| tr!("this instance has a single user — enable several users first"))?;
  let index = file.users.iter().position(|u| u.id == id).with_context(|| tr!("no user “{id}”", id = id))?;
  let user = file.users.remove(index);
  save(root, &file)?;
  Ok(user)
}

fn now() -> NaiveDateTime {
  Local::now().naive_local().with_nanosecond_zero()
}

trait WithoutNanos {
  fn with_nanosecond_zero(self) -> Self;
}

impl WithoutNanos for NaiveDateTime {
  fn with_nanosecond_zero(self) -> Self {
    chrono::Timelike::with_nanosecond(&self, 0).unwrap_or(self)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Activity, Project};

  #[test]
  fn keys_are_random_and_only_their_hash_is_kept() {
    let (a, b) = (new_key(), new_key());
    assert_eq!(a.len(), 48);
    assert_ne!(a, b);
    assert_eq!(hash_key(&a), hash_key(&format!(" {a}\n")), "spaces pasted around a key don't matter");
    assert_ne!(hash_key(&a), hash_key(&b));
  }

  #[test]
  fn enabling_moves_the_base_into_the_first_users_folder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let store = Store::open(root).unwrap();
    let mut settings = store.settings().unwrap();
    settings.employee_number = 4242;
    store.save_settings(&settings).unwrap();
    std::fs::write(root.join("entries").join("2026-09.json"), r#"{"month":"2026-09","entries":[]}"#).unwrap();

    let (user, key) = enable(root, "Arnaud Demion", None, Some("old-token")).unwrap();
    assert_eq!(user.id, "arnaud-demion");
    assert!(user.admin);
    assert_eq!(key, "old-token", "the existing token stays valid");
    assert_eq!(user.key_sha256, hash_key("old-token"));

    let moved = Store::open(user_dir(root, &user.id)).unwrap();
    assert_eq!(moved.settings().unwrap().employee_number, 4242);
    assert!(user_dir(root, &user.id).join("entries").join("2026-09.json").exists());
    assert!(!root.join("settings.json").exists() && !root.join("entries").exists());
    assert!(enable(root, "Again", None, None).is_err());
  }

  #[test]
  fn new_users_start_from_the_activities_and_can_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    enable(root, "Admin", Some("admin"), None).unwrap();
    let template = Catalog {
      projects: vec![Project { id: "p".into(), sagex_number: 1, name: "P".into(), color: "#000".into(), archived: false, exportable: true, note: None }],
      activities: vec![
        Activity { id: "ra-d".into(), sagex_number: 29, name: "Ra&D".into(), archived: false, note: None },
        Activity { id: "old".into(), sagex_number: 3, name: "Old".into(), archived: true, note: None },
      ],
      presets: Vec::new(),
    };

    let (alice, key) = add(root, NewUser { name: "Alice", template: Some(&template), ..Default::default() }).unwrap();
    assert_eq!(alice.id, "alice");
    assert!(!alice.admin);
    let catalog = Store::open(user_dir(root, "alice")).unwrap().catalog().unwrap();
    assert_eq!(catalog.activities.len(), 1, "archived activities are left out");
    assert!(catalog.projects.is_empty() && catalog.presets.is_empty());

    // A second Alice gets another folder; an explicit id already taken is refused.
    let (other, _) = add(root, NewUser { name: "Alice", ..Default::default() }).unwrap();
    assert_eq!(other.id, "alice-2");
    assert!(add(root, NewUser { name: "X", id: Some("alice"), ..Default::default() }).is_err());

    let (_, renewed) = renew_key(root, "alice").unwrap();
    assert_ne!(renewed, key);
    remove(root, "alice").unwrap();
    assert!(load(root).unwrap().unwrap().users.iter().all(|u| u.id != "alice"));
    assert!(user_dir(root, "alice").exists(), "the folder stays");
    // Its name stays taken while the folder is there.
    let (again, _) = add(root, NewUser { name: "Alice", ..Default::default() }).unwrap();
    assert_eq!(again.id, "alice-3");
  }
}
