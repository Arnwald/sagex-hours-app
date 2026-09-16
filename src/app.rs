//! État partagé du serveur.

use crate::store::Store;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
  pub store: Store,
  /// Sérialise les écritures : l'app est mono-utilisateur, un verrou global suffit
  /// et garantit qu'une lecture-modification-écriture n'est jamais entrelacée.
  pub write_lock: Arc<tokio::sync::Mutex<()>>,
  /// Jeton d'accès facultatif (`SAGEX_TOKEN`).
  pub auth_token: Option<String>,
  /// Horodatage de la dernière écriture faite par le serveur lui-même, pour que
  /// la surveillance du dossier ne rediffuse pas nos propres modifications.
  last_self_write: Arc<AtomicI64>,
}

impl AppState {
  pub fn new(store: Store, auth_token: Option<String>) -> Self {
    AppState {
      store,
      write_lock: Arc::new(tokio::sync::Mutex::new(())),
      auth_token,
      last_self_write: Arc::new(AtomicI64::new(0)),
    }
  }

  pub fn mark_self_write(&self) {
    self.last_self_write.store(now_millis(), Ordering::Relaxed);
  }

  /// Vrai si une écriture du serveur date de moins de `window_ms`.
  pub fn wrote_recently(&self, window_ms: i64) -> bool {
    now_millis() - self.last_self_write.load(Ordering::Relaxed) < window_ms
  }
}

fn now_millis() -> i64 {
  chrono::Utc::now().timestamp_millis()
}
