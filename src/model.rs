//! Modèle de données de la base « en clair ».
//!
//! Trois listes indépendantes (projets SageX, activités SageX, presets de saisie)
//! et des entrées regroupées par mois. Tous les types sont pensés pour donner un
//! JSON lisible et éditable à la main.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub type Id = String;

// ---------------------------------------------------------------------------
// Catalogue
// ---------------------------------------------------------------------------

/// Un projet SageX : le « N° projet Hesso » et son libellé.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
  pub id: Id,
  /// N° SageX. 0 est accepté pour un projet purement interne (voir `exportable`).
  pub sagex_number: u32,
  pub name: String,
  #[serde(default = "default_color")]
  pub color: String,
  #[serde(default)]
  pub archived: bool,
  /// `false` pour les projets qui restent dans l'app mais ne partent pas dans SageX
  /// (vacances, compensation, jours fériés).
  #[serde(default = "default_true")]
  pub exportable: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub note: Option<String>,
}

/// Une activité SageX. Liste unique, utilisable par n'importe quel projet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
  pub id: Id,
  pub sagex_number: u32,
  pub name: String,
  #[serde(default)]
  pub archived: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub note: Option<String>,
}

/// Raccourci de saisie : le couple (projet, activité) que l'on utilise souvent,
/// sous un nom court. Sert aussi de table de correspondance pour l'import Toggl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
  pub id: Id,
  pub label: String,
  pub project_id: Id,
  pub activity_id: Id,
  /// Commentaire pré-rempli lors de la création d'une entrée.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub comment: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub color: Option<String>,
  /// Couple (client, projet) Toggl correspondant, pour l'import de l'historique.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub toggl: Option<TogglRef>,
  #[serde(default)]
  pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TogglRef {
  pub client: String,
  pub project: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
  #[serde(default)]
  pub projects: Vec<Project>,
  #[serde(default)]
  pub activities: Vec<Activity>,
  #[serde(default)]
  pub presets: Vec<Preset>,
}

impl Catalog {
  pub fn project(&self, id: &str) -> Option<&Project> {
    self.projects.iter().find(|p| p.id == id)
  }
  pub fn activity(&self, id: &str) -> Option<&Activity> {
    self.activities.iter().find(|a| a.id == id)
  }
  pub fn preset_for_toggl(&self, client: &str, project: &str) -> Option<&Preset> {
    self.presets.iter().find(|p| match &p.toggl {
      Some(t) => t.client == client && t.project == project,
      None => false,
    })
  }
}

// ---------------------------------------------------------------------------
// Entrées
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
  Manual,
  Claude,
  Toggl,
  Timer,
  Import,
}

impl Default for Source {
  fn default() -> Self {
    Source::Manual
  }
}

/// Une saisie. `start`/`end` sont facultatifs : une entrée sans horaire reste
/// valide et s'affiche dans la bande « sans horaire » de la journée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
  pub id: Id,
  pub date: NaiveDate,
  #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_hhmm")]
  pub start: Option<NaiveTime>,
  #[serde(default, skip_serializing_if = "Option::is_none", with = "opt_hhmm")]
  pub end: Option<NaiveTime>,
  /// Durée en minutes. Fait foi, même quand start/end sont renseignés.
  pub minutes: u32,
  pub project_id: Id,
  pub activity_id: Id,
  #[serde(default)]
  pub comment: String,
  #[serde(default)]
  pub source: Source,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub toggl_id: Option<i64>,
  pub created_at: NaiveDateTime,
  pub updated_at: NaiveDateTime,
}

impl Entry {
  pub fn new_id() -> Id {
    ulid::Ulid::new().to_string()
  }

  pub fn now() -> NaiveDateTime {
    Local::now().naive_local().with_nanosecond(0).unwrap_or_else(|| Local::now().naive_local())
  }

  /// Heure de fin effective, déduite de la durée si `end` est absent.
  pub fn effective_end(&self) -> Option<NaiveTime> {
    match (self.start, self.end) {
      (_, Some(e)) => Some(e),
      (Some(s), None) => Some(s + chrono::Duration::minutes(self.minutes as i64)),
      _ => None,
    }
  }
}

/// Contenu d'un fichier `entries/YYYY-MM.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthFile {
  pub month: YearMonth,
  #[serde(default)]
  pub entries: Vec<Entry>,
}

impl MonthFile {
  pub fn empty(month: YearMonth) -> Self {
    MonthFile { month, entries: Vec::new() }
  }
}

/// Chronomètre en cours (`timer.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningTimer {
  pub project_id: Id,
  pub activity_id: Id,
  #[serde(default)]
  pub comment: String,
  pub started_at: NaiveDateTime,
}

// ---------------------------------------------------------------------------
// YearMonth
// ---------------------------------------------------------------------------

/// Un mois calendaire, sérialisé `"YYYY-MM"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YearMonth {
  pub year: i32,
  pub month: u32,
}

impl YearMonth {
  pub fn of(date: NaiveDate) -> Self {
    YearMonth { year: date.year(), month: date.month() }
  }

  pub fn first_day(&self) -> NaiveDate {
    NaiveDate::from_ymd_opt(self.year, self.month, 1).expect("mois valide")
  }

  pub fn last_day(&self) -> NaiveDate {
    self.next().first_day().pred_opt().expect("date valide")
  }

  pub fn next(&self) -> Self {
    if self.month == 12 {
      YearMonth { year: self.year + 1, month: 1 }
    } else {
      YearMonth { year: self.year, month: self.month + 1 }
    }
  }

  pub fn contains(&self, date: NaiveDate) -> bool {
    date.year() == self.year && date.month() == self.month
  }

  /// Tous les mois de `from` à `to` inclus.
  pub fn range(from: NaiveDate, to: NaiveDate) -> Vec<YearMonth> {
    let mut out = Vec::new();
    let mut cur = YearMonth::of(from);
    let last = YearMonth::of(to);
    while cur <= last {
      out.push(cur);
      cur = cur.next();
    }
    out
  }
}

impl fmt::Display for YearMonth {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{:04}-{:02}", self.year, self.month)
  }
}

#[derive(Debug, thiserror::Error)]
#[error("mois invalide « {0} », format attendu YYYY-MM")]
pub struct ParseYearMonthError(String);

impl FromStr for YearMonth {
  type Err = ParseYearMonthError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let (y, m) = s.split_once('-').ok_or_else(|| ParseYearMonthError(s.to_string()))?;
    let year: i32 = y.parse().map_err(|_| ParseYearMonthError(s.to_string()))?;
    let month: u32 = m.parse().map_err(|_| ParseYearMonthError(s.to_string()))?;
    if !(1..=12).contains(&month) {
      return Err(ParseYearMonthError(s.to_string()));
    }
    Ok(YearMonth { year, month })
  }
}

impl Serialize for YearMonth {
  fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&self.to_string())
  }
}

impl<'de> Deserialize<'de> for YearMonth {
  fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
  }
}

// ---------------------------------------------------------------------------
// Helpers de sérialisation
// ---------------------------------------------------------------------------

fn default_color() -> String {
  "#6b7a8f".to_string()
}

fn default_true() -> bool {
  true
}

/// Heures au format `"HH:MM"` (et non `HH:MM:SS`), tolérant en lecture.
pub mod opt_hhmm {
  use chrono::NaiveTime;
  use serde::{Deserialize, Deserializer, Serializer};

  pub fn serialize<S: Serializer>(t: &Option<NaiveTime>, s: S) -> Result<S::Ok, S::Error> {
    match t {
      Some(t) => s.serialize_str(&t.format("%H:%M").to_string()),
      None => s.serialize_none(),
    }
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<NaiveTime>, D::Error> {
    let raw = Option::<String>::deserialize(d)?;
    match raw {
      None => Ok(None),
      Some(s) if s.trim().is_empty() => Ok(None),
      Some(s) => parse_hhmm(&s).map(Some).map_err(serde::de::Error::custom),
    }
  }

  pub fn parse_hhmm(s: &str) -> Result<NaiveTime, String> {
    let s = s.trim();
    for fmt in ["%H:%M:%S", "%H:%M", "%Hh%M", "%H h %M"] {
      if let Ok(t) = NaiveTime::parse_from_str(s, fmt) {
        return Ok(t);
      }
    }
    Err(format!("heure invalide « {s} », format attendu HH:MM"))
  }
}

use chrono::Timelike;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn yearmonth_roundtrip() {
    let ym: YearMonth = "2026-09".parse().unwrap();
    assert_eq!((ym.year, ym.month), (2026, 9));
    assert_eq!(ym.to_string(), "2026-09");
    assert_eq!(ym.first_day(), NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
    assert_eq!(ym.last_day(), NaiveDate::from_ymd_opt(2026, 9, 30).unwrap());
    assert_eq!(ym.next().to_string(), "2026-10");
    assert_eq!("2026-12".parse::<YearMonth>().unwrap().next().to_string(), "2027-01");
    assert!("2026-13".parse::<YearMonth>().is_err());
  }

  #[test]
  fn yearmonth_range_spans_years() {
    let months = YearMonth::range(
      NaiveDate::from_ymd_opt(2025, 11, 20).unwrap(),
      NaiveDate::from_ymd_opt(2026, 2, 3).unwrap(),
    );
    let labels: Vec<String> = months.iter().map(|m| m.to_string()).collect();
    assert_eq!(labels, vec!["2025-11", "2025-12", "2026-01", "2026-02"]);
  }

  #[test]
  fn entry_times_serialize_as_hhmm() {
    let entry = Entry {
      id: "01ABC".into(),
      date: NaiveDate::from_ymd_opt(2026, 9, 16).unwrap(),
      start: Some(NaiveTime::from_hms_opt(8, 30, 0).unwrap()),
      end: None,
      minutes: 90,
      project_id: "spl-base".into(),
      activity_id: "seance".into(),
      comment: "Daily".into(),
      source: Source::Manual,
      toggl_id: None,
      created_at: Entry::now(),
      updated_at: Entry::now(),
    };
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("\"start\":\"08:30\""), "{json}");
    assert!(!json.contains("\"end\""), "end absent quand None: {json}");

    let back: Entry = serde_json::from_str(&json).unwrap();
    assert_eq!(back.start, entry.start);
    assert_eq!(back.effective_end(), NaiveTime::from_hms_opt(10, 0, 0));
  }

  #[test]
  fn entry_accepts_loose_time_formats() {
    let json = r#"{"id":"1","date":"2026-09-16","start":"8:05","end":"09h45","minutes":100,
      "project_id":"p","activity_id":"a","created_at":"2026-09-16T08:00:00","updated_at":"2026-09-16T08:00:00"}"#;
    let e: Entry = serde_json::from_str(json).unwrap();
    assert_eq!(e.start, NaiveTime::from_hms_opt(8, 5, 0));
    assert_eq!(e.end, NaiveTime::from_hms_opt(9, 45, 0));
    assert_eq!(e.comment, "");
    assert_eq!(e.source, Source::Manual);
  }
}
