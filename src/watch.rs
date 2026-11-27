//! Watching the data folder.
//!
//! A change made outside the API (a JSON edited by hand, a `git checkout`, a
//! restored backup) is pushed to the browsers showing that base. The server's
//! own writes are ignored: they have already sent their own event. A change to
//! `users.json` reloads the list of users.

use crate::app::AppState;
use crate::store::Change;
use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Window during which a file event counts as the echo of a server write.
const SELF_WRITE_WINDOW_MS: i64 = 1500;

/// Starts watching. The value returned must be kept alive.
pub fn spawn(state: AppState) -> Result<Box<dyn std::any::Any + Send>> {
  let root = state.root().to_path_buf();
  let notifier = state.clone();
  let users_file = crate::users::users_path(&root);

  let mut debouncer = new_debouncer(Duration::from_millis(300), None, move |result: notify_debouncer_full::DebounceEventResult| match result {
    Ok(events) => {
      let paths = relevant_paths(&events);
      if paths.iter().any(|p| same_file(p, &users_file)) {
        if let Err(err) = notifier.reload_users() {
          tracing::warn!("users.json: {err:#}");
        }
      }
      let mut notified = Vec::new();
      for path in paths.iter().filter(|p| !same_file(p, &users_file)) {
        let Some(space) = notifier.space_of(path) else { continue };
        if space.wrote_recently(SELF_WRITE_WINDOW_MS) || notified.iter().any(|root: &PathBuf| root == space.store.root()) {
          continue;
        }
        tracing::debug!("outside change detected: {}", path.display());
        notified.push(space.store.root().to_path_buf());
        space.store.notify(Change::scope("all"));
      }
    }
    Err(errors) => tracing::warn!("watching the folder: {errors:?}"),
  })?;

  debouncer.watch(&root, RecursiveMode::Recursive)?;
  tracing::info!("watching {}", root.display());
  Ok(Box::new(debouncer))
}

fn same_file(a: &Path, b: &Path) -> bool {
  a == b || a.file_name() == b.file_name() && a.parent().and_then(|p| p.canonicalize().ok()) == b.parent().and_then(|p| p.canonicalize().ok())
}

/// Keeps only the files of the bases (neither temporary files nor backups).
fn relevant_paths(events: &[notify_debouncer_full::DebouncedEvent]) -> Vec<PathBuf> {
  let mut out: Vec<PathBuf> = events
    .iter()
    .flat_map(|event| event.paths.iter())
    .filter(|path| {
      let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
      name.ends_with(".json") && !name.ends_with(".json.tmp") && !path.components().any(|c| c.as_os_str() == "backups")
    })
    .cloned()
    .collect();
  out.sort();
  out.dedup();
  out
}
