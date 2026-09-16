//! Construction des lignes SageX et écriture du fichier xlsx.
//!
//! Les colonnes reproduisent exactement ce qu'attend l'importeur SageX :
//! A n° école, B n° collaborateur, C n° projet, D n° activité, E heures `HHhMM`,
//! F heures en centième (laissée vide), G date, H commentaire, I n° personne.

use crate::model::{Catalog, Entry, YearMonth};
use crate::settings::{Aggregate, CommentStyle, Settings};
use anyhow::Result;
use chrono::NaiveDate;
use rust_xlsxwriter::{Format, Workbook};
use serde::Serialize;
use std::collections::BTreeMap;

/// Une ligne du fichier d'import.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
  pub school_number: u32,
  pub employee_number: u32,
  pub project_number: u32,
  pub activity_number: u32,
  /// `"08h30"`, éventuellement au-delà de 99 h pour une ligne mensuelle.
  pub hours: String,
  pub date: NaiveDate,
  pub comment: String,
  pub person_number: u64,
  /// Minutes cumulées, pour les totaux et les contrôles.
  pub minutes: u32,
  /// Identifiants des saisies résumées par la ligne.
  pub entry_ids: Vec<String>,
}

/// Saisies écartées de l'export, avec la raison.
#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
  pub entry_id: String,
  pub date: NaiveDate,
  pub minutes: u32,
  pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Export {
  pub rows: Vec<Row>,
  pub skipped: Vec<Skipped>,
}

impl Export {
  pub fn total_minutes(&self) -> u32 {
    self.rows.iter().map(|r| r.minutes).sum()
  }
}

/// Regroupe les saisies selon le mode d'agrégation et produit les lignes SageX.
pub fn build(entries: &[Entry], catalog: &Catalog, settings: &Settings, month: Option<YearMonth>) -> Export {
  let mut skipped = Vec::new();
  // Clé de regroupement → (n° projet, n° activité, date de la ligne, saisies).
  let mut groups: BTreeMap<(u32, u32, NaiveDate), Vec<&Entry>> = BTreeMap::new();

  for entry in entries {
    let Some(project) = catalog.project(&entry.project_id) else {
      skipped.push(reject(entry, format!("projet « {} » absent du catalogue", entry.project_id)));
      continue;
    };
    let Some(activity) = catalog.activity(&entry.activity_id) else {
      skipped.push(reject(entry, format!("activité « {} » absente du catalogue", entry.activity_id)));
      continue;
    };
    if !project.exportable || project.sagex_number == 0 {
      skipped.push(reject(entry, format!("projet « {} » exclu de l'export", project.name)));
      continue;
    }
    if entry.minutes == 0 {
      skipped.push(reject(entry, "durée nulle".to_string()));
      continue;
    }

    let date = match settings.export.aggregate {
      // Une ligne mensuelle est datée du dernier jour du mois, comme l'exige la directive.
      Aggregate::Month => month.unwrap_or_else(|| YearMonth::of(entry.date)).last_day(),
      Aggregate::Day | Aggregate::None => entry.date,
    };
    groups.entry((project.sagex_number, activity.sagex_number, date)).or_default().push(entry);
  }

  let mut rows = Vec::new();
  for ((project_number, activity_number, date), mut group) in groups {
    // Ligne mensuelle : du plus récent au plus ancien, comme le faisait sagex-rs.
    // Ligne journalière : ordre chronologique, plus naturel à relire.
    match settings.export.aggregate {
      Aggregate::Month => group.sort_by(|a, b| b.date.cmp(&a.date).then(b.start.cmp(&a.start))),
      Aggregate::Day | Aggregate::None => group.sort_by(|a, b| a.date.cmp(&b.date).then(a.start.cmp(&b.start))),
    }

    // Sans agrégation, chaque saisie garde sa ligne.
    let chunks: Vec<Vec<&Entry>> = match settings.export.aggregate {
      Aggregate::None => group.into_iter().map(|e| vec![e]).collect(),
      _ => vec![group],
    };

    for chunk in chunks {
      let minutes: u32 = chunk.iter().map(|e| e.minutes).sum();
      let with_dates = settings.export.aggregate == Aggregate::Month;
      rows.push(Row {
        school_number: settings.school_number,
        employee_number: settings.employee_number,
        project_number,
        activity_number,
        hours: format_hours(minutes),
        date,
        comment: build_comment(&chunk, settings, with_dates),
        person_number: settings.person_number,
        minutes,
        entry_ids: chunk.iter().map(|e| e.id.clone()).collect(),
      });
    }
  }

  rows.sort_by(|a, b| a.date.cmp(&b.date).then(a.project_number.cmp(&b.project_number)).then(a.activity_number.cmp(&b.activity_number)));
  Export { rows, skipped }
}

fn reject(entry: &Entry, reason: String) -> Skipped {
  Skipped { entry_id: entry.id.clone(), date: entry.date, minutes: entry.minutes, reason }
}

/// `"08h30"`. Les lignes mensuelles peuvent dépasser 99 h.
pub fn format_hours(minutes: u32) -> String {
  format!("{:02}h{:02}", minutes / 60, minutes % 60)
}

/// Assemble la colonne « Commentaire » en respectant la limite varchar1000.
fn build_comment(entries: &[&Entry], settings: &Settings, with_dates: bool) -> String {
  let separator = &settings.export.comment_separator;
  let max = settings.export.comment_max_len;

  let detailed = if with_dates {
    entries
      .iter()
      .map(|e| {
        let date = e.date.format(&settings.export.comment_date_format);
        if e.comment.is_empty() {
          date.to_string()
        } else {
          format!("{date} - {}", e.comment)
        }
      })
      .collect::<Vec<_>>()
      .join(separator)
  } else {
    let mut seen = Vec::new();
    for entry in entries {
      if !entry.comment.is_empty() && !seen.contains(&entry.comment) {
        seen.push(entry.comment.clone());
      }
    }
    seen.join(separator)
  };

  let style = settings.export.comment_style;
  let needs_compacting = match style {
    CommentStyle::Detailed => false,
    CommentStyle::Compact => true,
    CommentStyle::Auto => detailed.chars().count() > max,
  };

  let text = if needs_compacting { compact_comment(entries, settings, with_dates) } else { detailed };
  truncate(text, max)
}

/// Regroupe les descriptions identiques : `"Draft biblio (30.07, 29.07, 24.07)"`.
fn compact_comment(entries: &[&Entry], settings: &Settings, with_dates: bool) -> String {
  let mut order: Vec<String> = Vec::new();
  let mut dates: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for entry in entries {
    let label = if entry.comment.is_empty() { "(sans commentaire)".to_string() } else { entry.comment.clone() };
    if !order.contains(&label) {
      order.push(label.clone());
    }
    let day = entry.date.format("%d.%m").to_string();
    let list = dates.entry(label).or_default();
    if !list.contains(&day) {
      list.push(day);
    }
  }

  order
    .into_iter()
    .map(|label| {
      if !with_dates {
        return label;
      }
      let days = dates.remove(&label).unwrap_or_default();
      match days.len() {
        0 => label,
        1 => format!("{label} ({})", days[0]),
        n => format!("{label} ({n}× : {})", days.join(", ")),
      }
    })
    .collect::<Vec<_>>()
    .join(&settings.export.comment_separator)
}

fn truncate(text: String, max: usize) -> String {
  if text.chars().count() <= max {
    return text;
  }
  let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
  out.push('…');
  out
}

/// Écrit le classeur et retourne ses octets.
pub fn to_xlsx(rows: &[Row], settings: &Settings) -> Result<Vec<u8>> {
  let mut workbook = Workbook::new();
  let sheet = workbook.add_worksheet();

  for (column, title) in settings.export.header.iter().enumerate() {
    sheet.write_string(0, column as u16, title)?;
  }

  let date_format = Format::new().set_num_format("dd.mm.yyyy");
  for (index, row) in rows.iter().enumerate() {
    let line = index as u32 + 1;
    sheet.write_number(line, 0, row.school_number as f64)?;
    sheet.write_number(line, 1, row.employee_number as f64)?;
    sheet.write_number(line, 2, row.project_number as f64)?;
    sheet.write_number(line, 3, row.activity_number as f64)?;
    sheet.write_string(line, 4, &row.hours)?;
    // Colonne F (« Heures en centième ») volontairement vide : SageX la calcule.
    sheet.write_number_with_format(line, 6, excel_serial(row.date), &date_format)?;
    sheet.write_string(line, 7, &row.comment)?;
    sheet.write_number(line, 8, row.person_number as f64)?;
  }

  sheet.set_column_width(7, 80)?;
  Ok(workbook.save_to_buffer()?)
}

/// Nombre de jours depuis l'origine des dates Excel (1899-12-30).
fn excel_serial(date: NaiveDate) -> f64 {
  let origin = NaiveDate::from_ymd_opt(1899, 12, 30).expect("origine Excel");
  (date - origin).num_days() as f64
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Activity, Preset, Project, Source};

  fn catalog() -> Catalog {
    Catalog {
      projects: vec![
        Project { id: "spl-base".into(), sagex_number: 89403, name: "SPL Base".into(), color: "#000".into(), archived: false, exportable: true, note: None },
        Project { id: "subice".into(), sagex_number: 129030, name: "SUBICE".into(), color: "#000".into(), archived: false, exportable: true, note: None },
        Project { id: "non-sagex".into(), sagex_number: 0, name: "Non SageX".into(), color: "#000".into(), archived: false, exportable: false, note: None },
      ],
      activities: vec![
        Activity { id: "seance".into(), sagex_number: 4, name: "Séance".into(), archived: false, note: None },
        Activity { id: "ra-d".into(), sagex_number: 29, name: "Ra&D".into(), archived: false, note: None },
      ],
      presets: Vec::<Preset>::new(),
    }
  }

  fn entry(date: &str, project: &str, activity: &str, minutes: u32, comment: &str) -> Entry {
    Entry {
      id: format!("{date}-{project}-{activity}-{comment}"),
      date: date.parse().unwrap(),
      start: None,
      end: None,
      minutes,
      project_id: project.into(),
      activity_id: activity.into(),
      comment: comment.into(),
      source: Source::Manual,
      toggl_id: None,
      created_at: Entry::now(),
      updated_at: Entry::now(),
    }
  }

  fn settings() -> Settings {
    let mut s = Settings::default();
    s.employee_number = 351339;
    s.person_number = 9008376769;
    s
  }

  #[test]
  fn monthly_aggregation_matches_the_sagex_rs_output() {
    let entries = vec![
      entry("2026-07-02", "spl-base", "seance", 30, "Daily"),
      entry("2026-07-03", "spl-base", "seance", 30, "Daily"),
      entry("2026-07-06", "subice", "ra-d", 480, "Bibliography"),
    ];
    let export = build(&entries, &catalog(), &settings(), Some("2026-07".parse().unwrap()));
    assert_eq!(export.rows.len(), 2);

    let spl = export.rows.iter().find(|r| r.project_number == 89403).unwrap();
    assert_eq!(spl.hours, "01h00");
    assert_eq!(spl.activity_number, 4);
    // Toutes les lignes portent le dernier jour du mois.
    assert_eq!(spl.date.to_string(), "2026-07-31");
    assert_eq!(spl.comment, "03.07.2026 - Daily | 02.07.2026 - Daily");
    assert_eq!(spl.school_number, 1);
    assert_eq!(spl.employee_number, 351339);
    assert_eq!(spl.person_number, 9008376769);

    let subice = export.rows.iter().find(|r| r.project_number == 129030).unwrap();
    assert_eq!(subice.hours, "08h00");
  }

  #[test]
  fn hours_beyond_ninety_nine_stay_well_formed() {
    assert_eq!(format_hours(9000), "150h00");
    assert_eq!(format_hours(90), "01h30");
    assert_eq!(format_hours(5), "00h05");
  }

  #[test]
  fn non_exportable_projects_are_skipped_with_a_reason() {
    let entries = vec![
      entry("2026-07-02", "non-sagex", "seance", 480, "Vacances"),
      entry("2026-07-03", "spl-base", "seance", 60, "Daily"),
    ];
    let export = build(&entries, &catalog(), &settings(), Some("2026-07".parse().unwrap()));
    assert_eq!(export.rows.len(), 1);
    assert_eq!(export.skipped.len(), 1);
    assert!(export.skipped[0].reason.contains("exclu de l'export"));
  }

  #[test]
  fn daily_aggregation_keeps_one_row_per_day() {
    let mut settings = settings();
    settings.export.aggregate = Aggregate::Day;
    let entries = vec![
      entry("2026-07-02", "spl-base", "seance", 30, "Daily"),
      entry("2026-07-02", "spl-base", "seance", 60, "Sprint"),
      entry("2026-07-03", "spl-base", "seance", 30, "Daily"),
    ];
    let export = build(&entries, &catalog(), &settings, None);
    assert_eq!(export.rows.len(), 2);
    let first = &export.rows[0];
    assert_eq!(first.date.to_string(), "2026-07-02");
    assert_eq!(first.hours, "01h30");
    // Pas de préfixe de date sur une ligne déjà datée du jour.
    assert_eq!(first.comment, "Daily | Sprint");
  }

  #[test]
  fn long_comments_are_compacted_then_capped() {
    let mut entries = Vec::new();
    for day in 1..=28 {
      entries.push(entry(&format!("2026-07-{day:02}"), "subice", "ra-d", 240, "Bibliographie et rédaction du rapport"));
    }
    let export = build(&entries, &catalog(), &settings(), Some("2026-07".parse().unwrap()));
    let comment = &export.rows[0].comment;
    assert!(comment.chars().count() <= 1000, "longueur {}", comment.chars().count());
    // Le mode « auto » a basculé en compact : une seule fois le libellé, puis les dates.
    assert!(comment.starts_with("Bibliographie et rédaction du rapport (28× : 28.07"), "{comment}");
  }

  #[test]
  fn xlsx_has_the_expected_shape() {
    let entries = vec![entry("2026-07-02", "spl-base", "seance", 90, "Daily")];
    let export = build(&entries, &catalog(), &settings(), Some("2026-07".parse().unwrap()));
    let bytes = to_xlsx(&export.rows, &settings()).unwrap();
    assert!(bytes.starts_with(b"PK"), "un xlsx est une archive zip");
    assert!(bytes.len() > 2000);
  }

  #[test]
  fn excel_serial_matches_the_reference_value() {
    // 46234 est la valeur écrite par sagex-rs pour le 31.07.2026.
    assert_eq!(excel_serial(NaiveDate::from_ymd_opt(2026, 7, 31).unwrap()), 46234.0);
  }
}
