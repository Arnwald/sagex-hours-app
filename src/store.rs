//! Accès à la base « en clair ».
//!
//! Les fichiers JSON sont la source de vérité : chaque lecture relit le disque,
//! si bien qu'une modification faite à la main (ou par Claude) est prise en
//! compte immédiatement, sans cache à invalider. Les écritures sont atomiques
//! (fichier temporaire puis `rename`) et re-trient les entrées pour garder des
//! diffs lisibles.

use crate::model::{Catalog, Entry, MonthFile, RunningTimer, YearMonth};
use crate::settings::Settings;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;

/// Évènement diffusé aux navigateurs connectés (SSE).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Change {
  /// `entries`, `catalog`, `settings`, `timer` ou `all`.
  pub scope: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub month: Option<String>,
}

impl Change {
  pub fn scope(scope: &str) -> Self {
    Change { scope: scope.to_string(), month: None }
  }
  pub fn month(month: YearMonth) -> Self {
    Change { scope: "entries".to_string(), month: Some(month.to_string()) }
  }
}

#[derive(Clone)]
pub struct Store {
  root: PathBuf,
  tx: broadcast::Sender<Change>,
}

impl Store {
  /// Ouvre (et initialise si nécessaire) la base dans `root`.
  pub fn open(root: impl AsRef<Path>) -> Result<Self> {
    let root = root.as_ref().to_path_buf();
    std::fs::create_dir_all(root.join("entries")).with_context(|| format!("création de {}", root.display()))?;
    std::fs::create_dir_all(root.join("backups"))?;
    let (tx, _) = broadcast::channel(64);
    let store = Store { root, tx };

    if !store.settings_path().exists() {
      store.save_settings(&Settings::default())?;
    }
    if !store.catalog_path().exists() {
      store.save_catalog(&Catalog::default())?;
    }
    Ok(store)
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  // -- diffusion ------------------------------------------------------------

  pub fn subscribe(&self) -> broadcast::Receiver<Change> {
    self.tx.subscribe()
  }

  pub fn notify(&self, change: Change) {
    let _ = self.tx.send(change);
  }

  // -- chemins --------------------------------------------------------------

  pub fn settings_path(&self) -> PathBuf {
    self.root.join("settings.json")
  }
  pub fn catalog_path(&self) -> PathBuf {
    self.root.join("catalog.json")
  }
  pub fn timer_path(&self) -> PathBuf {
    self.root.join("timer.json")
  }
  pub fn month_path(&self, month: YearMonth) -> PathBuf {
    self.root.join("entries").join(format!("{month}.json"))
  }
  pub fn backups_dir(&self) -> PathBuf {
    self.root.join("backups")
  }

  // -- settings -------------------------------------------------------------

  pub fn settings(&self) -> Result<Settings> {
    read_json(&self.settings_path()).map(|s| s.unwrap_or_default())
  }

  pub fn save_settings(&self, settings: &Settings) -> Result<()> {
    write_json(&self.settings_path(), settings)
  }

  // -- catalogue ------------------------------------------------------------

  pub fn catalog(&self) -> Result<Catalog> {
    read_json(&self.catalog_path()).map(|c| c.unwrap_or_default())
  }

  pub fn save_catalog(&self, catalog: &Catalog) -> Result<()> {
    write_json(&self.catalog_path(), catalog)
  }

  // -- chronomètre ----------------------------------------------------------

  pub fn timer(&self) -> Result<Option<RunningTimer>> {
    read_json(&self.timer_path())
  }

  pub fn save_timer(&self, timer: Option<&RunningTimer>) -> Result<()> {
    match timer {
      Some(t) => write_json(&self.timer_path(), t),
      None => {
        let path = self.timer_path();
        if path.exists() {
          std::fs::remove_file(&path).with_context(|| format!("suppression de {}", path.display()))?;
        }
        Ok(())
      }
    }
  }

  // -- entrées --------------------------------------------------------------

  pub fn month(&self, month: YearMonth) -> Result<MonthFile> {
    Ok(read_json(&self.month_path(month))?.unwrap_or_else(|| MonthFile::empty(month)))
  }

  /// Écrit un mois. Un mois vide est supprimé plutôt que laissé en place.
  pub fn save_month(&self, file: &mut MonthFile) -> Result<()> {
    let path = self.month_path(file.month);
    if file.entries.is_empty() {
      if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("suppression de {}", path.display()))?;
      }
      return Ok(());
    }
    file.entries.sort_by(|a, b| {
      a.date.cmp(&b.date).then(a.start.cmp(&b.start)).then(a.created_at.cmp(&b.created_at)).then(a.id.cmp(&b.id))
    });
    write_json(&path, file)
  }

  /// Mois pour lesquels un fichier existe, du plus ancien au plus récent.
  pub fn months(&self) -> Result<Vec<YearMonth>> {
    let dir = self.root.join("entries");
    let mut out = Vec::new();
    if !dir.exists() {
      return Ok(out);
    }
    for entry in std::fs::read_dir(&dir).with_context(|| format!("lecture de {}", dir.display()))? {
      let entry = entry?;
      let name = entry.file_name();
      let name = name.to_string_lossy();
      if let Some(stem) = name.strip_suffix(".json") {
        if let Ok(ym) = stem.parse::<YearMonth>() {
          out.push(ym);
        }
      }
    }
    out.sort();
    Ok(out)
  }

  pub fn entries_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for month in YearMonth::range(from, to) {
      for entry in self.month(month)?.entries {
        if entry.date >= from && entry.date <= to {
          out.push(entry);
        }
      }
    }
    out.sort_by(|a, b| a.date.cmp(&b.date).then(a.start.cmp(&b.start)).then(a.id.cmp(&b.id)));
    Ok(out)
  }

  /// Retrouve une entrée par son identifiant, en partant des mois les plus récents.
  pub fn find_entry(&self, id: &str) -> Result<Option<(YearMonth, Entry)>> {
    let mut months = self.months()?;
    months.reverse();
    for month in months {
      if let Some(entry) = self.month(month)?.entries.into_iter().find(|e| e.id == id) {
        return Ok(Some((month, entry)));
      }
    }
    Ok(None)
  }

  /// Ajoute des entrées, en les répartissant dans le bon fichier mensuel.
  pub fn insert_entries(&self, entries: Vec<Entry>) -> Result<Vec<Entry>> {
    let mut touched: std::collections::BTreeMap<YearMonth, Vec<Entry>> = Default::default();
    for entry in &entries {
      touched.entry(YearMonth::of(entry.date)).or_default().push(entry.clone());
    }
    for (month, new_entries) in touched {
      let mut file = self.month(month)?;
      file.entries.extend(new_entries);
      self.save_month(&mut file)?;
      self.notify(Change::month(month));
    }
    Ok(entries)
  }

  pub fn delete_entry(&self, id: &str) -> Result<Option<Entry>> {
    let Some((month, _)) = self.find_entry(id)? else {
      return Ok(None);
    };
    let mut file = self.month(month)?;
    let position = file.entries.iter().position(|e| e.id == id);
    let removed = position.map(|i| file.entries.remove(i));
    self.save_month(&mut file)?;
    self.notify(Change::month(month));
    Ok(removed)
  }

  /// Remplace une entrée, en la déplaçant de fichier si sa date a changé de mois.
  pub fn replace_entry(&self, previous_month: YearMonth, entry: Entry) -> Result<()> {
    let new_month = YearMonth::of(entry.date);
    let mut file = self.month(previous_month)?;
    file.entries.retain(|e| e.id != entry.id);
    if new_month == previous_month {
      file.entries.push(entry);
      self.save_month(&mut file)?;
    } else {
      self.save_month(&mut file)?;
      let mut target = self.month(new_month)?;
      target.entries.push(entry);
      self.save_month(&mut target)?;
      self.notify(Change::month(new_month));
    }
    self.notify(Change::month(previous_month));
    Ok(())
  }
}

// ---------------------------------------------------------------------------
// Entrées/sorties JSON
// ---------------------------------------------------------------------------

/// Lit un JSON. Retourne `None` si le fichier n'existe pas (ou est vide).
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
  if !path.exists() {
    return Ok(None);
  }
  let raw = std::fs::read_to_string(path).with_context(|| format!("lecture de {}", path.display()))?;
  if raw.trim().is_empty() {
    return Ok(None);
  }
  let value = serde_json::from_str(&raw).with_context(|| format!("JSON invalide dans {}", path.display()))?;
  Ok(Some(value))
}

/// Écriture atomique : fichier temporaire voisin puis `rename`.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).with_context(|| format!("création de {}", parent.display()))?;
  }
  let mut json = serde_json::to_string_pretty(value)?;
  json.push('\n');
  let tmp = path.with_extension("json.tmp");
  std::fs::write(&tmp, json).with_context(|| format!("écriture de {}", tmp.display()))?;
  std::fs::rename(&tmp, path).with_context(|| format!("remplacement de {}", path.display()))?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Entry, Source};
  use chrono::NaiveTime;

  fn entry(date: &str, id: &str) -> Entry {
    Entry {
      id: id.to_string(),
      date: date.parse().unwrap(),
      start: Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
      end: None,
      minutes: 60,
      project_id: "spl-base".into(),
      activity_id: "seance".into(),
      comment: "Daily".into(),
      source: Source::Manual,
      toggl_id: None,
      created_at: Entry::now(),
      updated_at: Entry::now(),
    }
  }

  #[test]
  fn roundtrip_through_disk() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();

    store.insert_entries(vec![entry("2026-09-16", "b"), entry("2026-09-02", "a"), entry("2026-10-01", "c")]).unwrap();

    // Les entrées sont rangées dans le fichier du mois correspondant, triées par date.
    let september = store.month("2026-09".parse().unwrap()).unwrap();
    assert_eq!(september.entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert_eq!(store.month("2026-10".parse().unwrap()).unwrap().entries.len(), 1);
    assert_eq!(store.months().unwrap().len(), 2);

    // Le JSON écrit est relisible tel quel.
    let raw = std::fs::read_to_string(store.month_path("2026-09".parse().unwrap())).unwrap();
    assert!(raw.contains("\"month\": \"2026-09\""), "{raw}");
    assert!(raw.contains("\"start\": \"09:00\""), "{raw}");

    let range = store
      .entries_between("2026-09-10".parse().unwrap(), "2026-10-05".parse().unwrap())
      .unwrap();
    assert_eq!(range.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["b", "c"]);
  }

  #[test]
  fn move_entry_across_months_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.insert_entries(vec![entry("2026-09-30", "x")]).unwrap();

    let (month, mut found) = store.find_entry("x").unwrap().unwrap();
    assert_eq!(month.to_string(), "2026-09");
    found.date = "2026-10-01".parse().unwrap();
    store.replace_entry(month, found).unwrap();

    assert!(store.month("2026-09".parse().unwrap()).unwrap().entries.is_empty());
    assert_eq!(store.find_entry("x").unwrap().unwrap().0.to_string(), "2026-10");
    // Un mois vidé ne laisse pas de fichier derrière lui.
    assert!(!store.month_path("2026-09".parse().unwrap()).exists());

    assert!(store.delete_entry("x").unwrap().is_some());
    assert!(store.find_entry("x").unwrap().is_none());
    assert!(store.delete_entry("x").unwrap().is_none());
  }

  #[test]
  fn hand_written_file_is_readable() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    std::fs::write(
      store.month_path("2026-09".parse().unwrap()),
      r#"{ "month": "2026-09", "entries": [
        { "id": "manuel", "date": "2026-09-16", "minutes": 240, "project_id": "p", "activity_id": "a",
          "created_at": "2026-09-16T08:00:00", "updated_at": "2026-09-16T08:00:00" } ] }"#,
    )
    .unwrap();
    let entries = store.month("2026-09".parse().unwrap()).unwrap().entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].minutes, 240);
    assert!(entries[0].start.is_none());
  }
}
