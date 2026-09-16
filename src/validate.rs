//! Totaux et contrôles d'un mois avant envoi dans SageX.

use crate::export;
use crate::model::{Catalog, Entry, YearMonth};
use crate::settings::Settings;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  /// Bloque l'export : SageX refuserait la ligne ou la fausserait.
  Error,
  /// À vérifier, mais l'export reste possible.
  Warning,
  Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
  pub severity: Severity,
  pub code: &'static str,
  pub message: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub date: Option<NaiveDate>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub entry_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Total {
  pub id: String,
  pub name: String,
  pub sagex_number: u32,
  pub minutes: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayTotal {
  pub date: NaiveDate,
  pub minutes: u32,
  pub weekend: bool,
  pub holiday: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
  pub month: String,
  pub from: NaiveDate,
  pub to: NaiveDate,
  /// Total saisi, export exclu compris.
  pub total_minutes: u32,
  /// Total qui partira dans SageX.
  pub exported_minutes: u32,
  /// Total des projets marqués « non exportable » (vacances, récupération…).
  pub excluded_minutes: u32,
  /// Attendu = jours ouvrables × imputation quotidienne × taux d'activité.
  pub expected_minutes: u32,
  pub working_days: usize,
  pub holidays: Vec<DayTotal>,
  pub rows: usize,
  pub by_project: Vec<Total>,
  pub by_activity: Vec<Total>,
  pub by_day: Vec<DayTotal>,
  pub issues: Vec<Issue>,
}

impl Report {
  pub fn has_errors(&self) -> bool {
    self.issues.iter().any(|i| i.severity == Severity::Error)
  }
}

/// Journée au-delà de laquelle on signale une saisie suspecte.
const LONG_DAY_MINUTES: u32 = 12 * 60;

pub fn report(month: YearMonth, entries: &[Entry], catalog: &Catalog, settings: &Settings) -> Report {
  let from = month.first_day();
  let to = month.last_day();
  let export = export::build(entries, catalog, settings, Some(month));
  let mut issues = Vec::new();

  // -- cohérence des réglages ----------------------------------------------
  if settings.employee_number == 0 || settings.person_number == 0 {
    issues.push(Issue {
      severity: Severity::Error,
      code: "settings-incomplets",
      message: "n° de collaborateur ou n° de personne manquant dans les réglages : l'import SageX échouerait".to_string(),
      date: None,
      entry_ids: Vec::new(),
    });
  }

  // -- références et exclusions --------------------------------------------
  for skipped in &export.skipped {
    let severity = if skipped.reason.contains("absent") { Severity::Error } else { Severity::Info };
    issues.push(Issue {
      severity,
      code: if severity == Severity::Error { "reference-inconnue" } else { "hors-export" },
      message: format!("{} ({}) — {}", skipped.date, export::format_hours(skipped.minutes), skipped.reason),
      date: Some(skipped.date),
      entry_ids: vec![skipped.entry_id.clone()],
    });
  }

  // -- longueur des commentaires -------------------------------------------
  let max = settings.export.comment_max_len;
  for row in &export.rows {
    let length = row.comment.chars().count();
    if length >= max {
      issues.push(Issue {
        severity: Severity::Error,
        code: "commentaire-trop-long",
        message: format!(
          "le commentaire du projet {} / activité {} atteint la limite de {max} caractères et sera tronqué — passe le style de commentaire en « compact » ou découpe la ligne",
          row.project_number, row.activity_number
        ),
        date: Some(row.date),
        entry_ids: row.entry_ids.clone(),
      });
    }
  }

  // -- saisies elles-mêmes --------------------------------------------------
  for entry in entries {
    if !month.contains(entry.date) {
      issues.push(Issue {
        severity: Severity::Error,
        code: "hors-mois",
        message: format!("saisie datée du {} alors que le mois exporté est {month}", entry.date),
        date: Some(entry.date),
        entry_ids: vec![entry.id.clone()],
      });
    }
    if entry.minutes == 0 {
      issues.push(Issue {
        severity: Severity::Warning,
        code: "duree-nulle",
        message: format!("saisie sans durée le {}", entry.date),
        date: Some(entry.date),
        entry_ids: vec![entry.id.clone()],
      });
    }
  }

  // -- chevauchements et journées longues ----------------------------------
  let mut per_day: BTreeMap<NaiveDate, Vec<&Entry>> = BTreeMap::new();
  for entry in entries {
    per_day.entry(entry.date).or_default().push(entry);
  }

  for (date, day_entries) in &per_day {
    let minutes: u32 = day_entries.iter().map(|e| e.minutes).sum();
    if minutes > LONG_DAY_MINUTES {
      issues.push(Issue {
        severity: Severity::Warning,
        code: "journee-longue",
        message: format!("{} imputées le {date}", export::format_hours(minutes)),
        date: Some(*date),
        entry_ids: day_entries.iter().map(|e| e.id.clone()).collect(),
      });
    }

    let mut timed: Vec<(&Entry, chrono::NaiveTime, chrono::NaiveTime)> = day_entries
      .iter()
      .filter_map(|e| e.start.map(|start| (*e, start, e.effective_end().unwrap_or(start))))
      .collect();
    timed.sort_by_key(|(_, start, _)| *start);
    for pair in timed.windows(2) {
      let (first, _, first_end) = pair[0];
      let (second, second_start, _) = pair[1];
      if second_start < first_end {
        issues.push(Issue {
          severity: Severity::Warning,
          code: "chevauchement",
          message: format!(
            "le {date}, « {} » finit à {} mais « {} » commence à {}",
            label(first), first_end.format("%H:%M"), label(second), second_start.format("%H:%M")
          ),
          date: Some(*date),
          entry_ids: vec![first.id.clone(), second.id.clone()],
        });
      }
    }
  }

  // -- totaux ---------------------------------------------------------------
  let holidays = holidays(month.year, settings);
  let mut by_day = Vec::new();
  let mut working_days = 0;
  let mut cursor = from;
  while cursor <= to {
    let weekend = matches!(cursor.weekday(), Weekday::Sat | Weekday::Sun);
    let holiday = holidays.iter().find(|(date, _)| *date == cursor).map(|(_, name)| name.to_string());
    if !weekend && holiday.is_none() {
      working_days += 1;
    }
    by_day.push(DayTotal {
      date: cursor,
      minutes: per_day.get(&cursor).map(|e| e.iter().map(|e| e.minutes).sum()).unwrap_or(0),
      weekend,
      holiday,
    });
    cursor += Duration::days(1);
  }

  let total_minutes: u32 = entries.iter().map(|e| e.minutes).sum();
  let exported_minutes = export.total_minutes();
  let excluded_minutes = total_minutes.saturating_sub(exported_minutes);
  let expected_minutes = working_days as u32 * settings.expected_minutes_per_day();

  // Écart supérieur à une demi-journée : cela vaut un coup d'œil.
  let gap = total_minutes as i64 - expected_minutes as i64;
  if gap.abs() > (settings.expected_minutes_per_day() / 2) as i64 {
    issues.push(Issue {
      severity: Severity::Warning,
      code: "total-inattendu",
      message: format!(
        "{} saisies pour {} attendues ({working_days} jours ouvrables) — écart de {}{}",
        export::format_hours(total_minutes),
        export::format_hours(expected_minutes),
        if gap > 0 { "+" } else { "−" },
        export::format_hours(gap.unsigned_abs() as u32)
      ),
      date: None,
      entry_ids: Vec::new(),
    });
  }

  Report {
    month: month.to_string(),
    from,
    to,
    total_minutes,
    exported_minutes,
    excluded_minutes,
    expected_minutes,
    working_days,
    holidays: by_day.iter().filter(|d| d.holiday.is_some()).cloned().collect(),
    rows: export.rows.len(),
    by_project: totals(entries, catalog, true),
    by_activity: totals(entries, catalog, false),
    by_day,
    issues,
  }
}

fn label(entry: &Entry) -> String {
  if entry.comment.is_empty() {
    entry.project_id.clone()
  } else {
    entry.comment.clone()
  }
}

fn totals(entries: &[Entry], catalog: &Catalog, by_project: bool) -> Vec<Total> {
  let mut sums: BTreeMap<String, u32> = BTreeMap::new();
  for entry in entries {
    let key = if by_project { &entry.project_id } else { &entry.activity_id };
    *sums.entry(key.clone()).or_default() += entry.minutes;
  }
  let mut out: Vec<Total> = sums
    .into_iter()
    .map(|(id, minutes)| {
      let (name, number) = if by_project {
        catalog.project(&id).map(|p| (p.name.clone(), p.sagex_number)).unwrap_or((format!("{id} (inconnu)"), 0))
      } else {
        catalog.activity(&id).map(|a| (a.name.clone(), a.sagex_number)).unwrap_or((format!("{id} (inconnue)"), 0))
      };
      Total { id, name, sagex_number: number, minutes }
    })
    .collect();
  out.sort_by(|a, b| b.minutes.cmp(&a.minutes));
  out
}

// ---------------------------------------------------------------------------
// Jours fériés
// ---------------------------------------------------------------------------

/// Fériés valaisans de l'année, plus ceux ajoutés dans les réglages.
///
/// La liste est indicative : elle sert au calcul du total attendu et reste
/// ajustable via `extra_holidays`.
pub fn holidays(year: i32, settings: &Settings) -> Vec<(NaiveDate, String)> {
  let mut out: Vec<(NaiveDate, String)> = Vec::new();
  if settings.compute_holidays {
    let easter = easter_sunday(year);
    let fixed: [(u32, u32, &str); 7] = [
      (1, 1, "Nouvel An"),
      (3, 19, "Saint-Joseph"),
      (8, 1, "Fête nationale"),
      (8, 15, "Assomption"),
      (11, 1, "Toussaint"),
      (12, 8, "Immaculée Conception"),
      (12, 25, "Noël"),
    ];
    for (month, day, name) in fixed {
      if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
        out.push((date, name.to_string()));
      }
    }
    for (offset, name) in [(1, "Lundi de Pâques"), (39, "Ascension"), (50, "Lundi de Pentecôte"), (60, "Fête-Dieu")] {
      out.push((easter + Duration::days(offset), name.to_string()));
    }
  }
  for date in &settings.extra_holidays {
    if date.year() == year && !out.iter().any(|(d, _)| d == date) {
      out.push((*date, "Férié (réglages)".to_string()));
    }
  }
  out.sort_by_key(|(date, _)| *date);
  out
}

/// Comput grégorien (algorithme de Meeus/Jones/Butcher).
pub fn easter_sunday(year: i32) -> NaiveDate {
  let a = year % 19;
  let b = year / 100;
  let c = year % 100;
  let d = b / 4;
  let e = b % 4;
  let f = (b + 8) / 25;
  let g = (b - f + 1) / 3;
  let h = (19 * a + b - d - g + 15) % 30;
  let i = c / 4;
  let k = c % 4;
  let l = (32 + 2 * e + 2 * i - h - k) % 7;
  let m = (a + 11 * h + 22 * l) / 451;
  let month = (h + l - 7 * m + 114) / 31;
  let day = ((h + l - 7 * m + 114) % 31) + 1;
  NaiveDate::from_ymd_opt(year, month as u32, day as u32).expect("date de Pâques valide")
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Activity, Project, Source};

  fn catalog() -> Catalog {
    Catalog {
      projects: vec![
        Project { id: "spl-base".into(), sagex_number: 89403, name: "SPL Base".into(), color: "#000".into(), archived: false, exportable: true, note: None },
        Project { id: "conges".into(), sagex_number: 0, name: "Congés".into(), color: "#000".into(), archived: false, exportable: false, note: None },
      ],
      activities: vec![Activity { id: "seance".into(), sagex_number: 4, name: "Séance".into(), archived: false, note: None }],
      presets: Vec::new(),
    }
  }

  fn settings() -> Settings {
    let mut s = Settings::default();
    s.employee_number = 351339;
    s.person_number = 9008376769;
    s
  }

  fn entry(date: &str, start: Option<&str>, minutes: u32, project: &str) -> Entry {
    Entry {
      id: format!("{date}-{}-{minutes}", start.unwrap_or("na")),
      date: date.parse().unwrap(),
      start: start.map(|s| s.parse().unwrap()),
      end: None,
      minutes,
      project_id: project.into(),
      activity_id: "seance".into(),
      comment: "Travail".into(),
      source: Source::Manual,
      toggl_id: None,
      created_at: Entry::now(),
      updated_at: Entry::now(),
    }
  }

  fn codes(report: &Report) -> Vec<&str> {
    report.issues.iter().map(|i| i.code).collect()
  }

  #[test]
  fn easter_and_holidays_are_correct_for_2026() {
    assert_eq!(easter_sunday(2026), NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
    assert_eq!(easter_sunday(2025), NaiveDate::from_ymd_opt(2025, 4, 20).unwrap());

    let days = holidays(2026, &settings());
    let ascension = days.iter().find(|(_, name)| name == "Ascension").unwrap().0;
    assert_eq!(ascension, NaiveDate::from_ymd_opt(2026, 5, 14).unwrap());
    assert!(days.iter().any(|(d, n)| n == "Saint-Joseph" && *d == NaiveDate::from_ymd_opt(2026, 3, 19).unwrap()));
  }

  #[test]
  fn overlapping_entries_are_flagged() {
    let entries = vec![entry("2026-09-14", Some("08:00"), 120, "spl-base"), entry("2026-09-14", Some("09:00"), 60, "spl-base")];
    let report = report("2026-09".parse().unwrap(), &entries, &catalog(), &settings());
    assert!(codes(&report).contains(&"chevauchement"), "{:?}", codes(&report));
  }

  #[test]
  fn adjacent_entries_do_not_overlap() {
    let entries = vec![entry("2026-09-14", Some("08:00"), 60, "spl-base"), entry("2026-09-14", Some("09:00"), 60, "spl-base")];
    let report = report("2026-09".parse().unwrap(), &entries, &catalog(), &settings());
    assert!(!codes(&report).contains(&"chevauchement"), "{:?}", codes(&report));
  }

  #[test]
  fn non_exportable_hours_are_counted_apart() {
    let entries = vec![entry("2026-09-14", None, 480, "spl-base"), entry("2026-09-15", None, 480, "conges")];
    let report = report("2026-09".parse().unwrap(), &entries, &catalog(), &settings());
    assert_eq!(report.total_minutes, 960);
    assert_eq!(report.exported_minutes, 480);
    assert_eq!(report.excluded_minutes, 480);
    assert!(codes(&report).contains(&"hors-export"));
  }

  #[test]
  fn unknown_reference_is_an_error() {
    let entries = vec![entry("2026-09-14", None, 60, "projet-supprime")];
    let report = report("2026-09".parse().unwrap(), &entries, &catalog(), &settings());
    assert!(report.has_errors());
    assert!(codes(&report).contains(&"reference-inconnue"));
  }

  #[test]
  fn entry_outside_the_month_is_an_error() {
    let entries = vec![entry("2026-10-01", None, 60, "spl-base")];
    let report = report("2026-09".parse().unwrap(), &entries, &catalog(), &settings());
    assert!(codes(&report).contains(&"hors-mois"));
  }

  #[test]
  fn expected_hours_account_for_weekends_and_holidays() {
    // Septembre 2026 : 22 jours ouvrables, aucun férié valaisan.
    let september = report("2026-09".parse().unwrap(), &[], &catalog(), &settings());
    assert_eq!(september.working_days, 22);
    assert_eq!(september.expected_minutes, 22 * 492);
    assert!(september.holidays.is_empty());

    // Août 2026 : le 1er tombe un samedi, le 15 un samedi également.
    let august = report("2026-08".parse().unwrap(), &[], &catalog(), &settings());
    assert_eq!(august.working_days, 21);
  }

  #[test]
  fn missing_identity_blocks_the_export() {
    let report = report("2026-09".parse().unwrap(), &[], &catalog(), &Settings::default());
    assert!(report.has_errors());
    assert!(codes(&report).contains(&"settings-incomplets"));
  }
}
