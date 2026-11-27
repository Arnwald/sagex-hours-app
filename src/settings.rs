//! `settings.json`: the employee's identity, calculation rules, export options.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
  /// "N° Interne Collaborateur" (SageX → Paramètres).
  pub employee_number: u32,
  /// "I_PERSONNE" (SageX → Paramètres).
  pub person_number: u64,
  /// School number, 1 for HES-SO Valais.
  pub school_number: u32,
  /// Activity rate, 1.0 = 100 %.
  pub activity_rate: f64,
  /// Average daily hours for a 100 % rate, `"HH:MM"`. 8h12 per the 2026 directive.
  pub hours_per_day: String,
  /// Extra or substitute public holidays (Valais holidays are computed).
  #[serde(default)]
  pub extra_holidays: Vec<NaiveDate>,
  /// Turns off the automatic computation of Valais public holidays.
  pub compute_holidays: bool,
  pub export: ExportSettings,
  pub toggl: TogglSettings,
  pub ui: UiSettings,
  /// Date of the last full backup downloaded.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub last_backup: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportSettings {
  pub aggregate: Aggregate,
  /// How the "Commentaire" column is laid out when aggregating.
  pub comment_style: CommentStyle,
  /// Format of the dates prefixing each aggregated comment.
  pub comment_date_format: String,
  pub comment_separator: String,
  /// SageX's "Commentaire" column is a varchar1000.
  pub comment_max_len: usize,
  pub header: Vec<String>,
  /// Name of the produced file; `{from}` and `{to}` are substituted.
  pub file_name: String,
}

/// How to condense the descriptions grouped on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentStyle {
  /// One mention per entry: `"30.07.2026 - Daily | 29.07.2026 - Daily"`.
  Detailed,
  /// Identical descriptions grouped: `"Daily (2× : 30.07, 29.07)"`.
  Compact,
  /// Detailed, compacted only when the varchar1000 limit is exceeded.
  Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregate {
  /// One line per (project, activity) and month, dated the last day of the month.
  Month,
  /// One line per (project, activity) and day.
  Day,
  /// One line per entry.
  None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TogglSettings {
  /// Can also be given by the `TOGGL_API_TOKEN` environment variable.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub api_token: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
  /// First hour shown in the week grid.
  pub day_start_hour: u32,
  pub day_end_hour: u32,
  /// Grid step, in minutes.
  pub slot_minutes: u32,
  /// Default duration of an entry created with one click.
  pub default_minutes: u32,
  /// Backup reminder after N days without a backup.
  pub backup_reminder_days: i64,
}

impl Default for Settings {
  fn default() -> Self {
    Settings {
      employee_number: 0,
      person_number: 0,
      school_number: 1,
      activity_rate: 1.0,
      hours_per_day: "08:12".to_string(),
      extra_holidays: Vec::new(),
      compute_holidays: true,
      export: ExportSettings::default(),
      toggl: TogglSettings::default(),
      ui: UiSettings::default(),
      last_backup: None,
    }
  }
}

impl Default for ExportSettings {
  fn default() -> Self {
    ExportSettings {
      aggregate: Aggregate::Month,
      comment_style: CommentStyle::Auto,
      comment_date_format: "%d.%m.%Y".to_string(),
      comment_separator: " | ".to_string(),
      comment_max_len: 1000,
      // These labels are what SageX's importer expects: do not translate them.
      header: vec![
        "N° école (number5)".to_string(),
        "N° Interne Collaborateur (number 5)".to_string(),
        "N° projet Hesso (number 5)".to_string(),
        "N° activité (number 5)".to_string(),
        "Heures".to_string(),
        "Heures (en centième)".to_string(),
        "Date (jj.mm.aaaa)".to_string(),
        "Commentaire (varchar1000)".to_string(),
        "I_PERSONNE (varchar15)".to_string(),
      ],
      file_name: "sagex_{from}_to_{to}".to_string(),
    }
  }
}

impl Default for UiSettings {
  fn default() -> Self {
    UiSettings {
      day_start_hour: 6,
      day_end_hour: 22,
      slot_minutes: 15,
      default_minutes: 60,
      backup_reminder_days: 7,
    }
  }
}

impl Settings {
  /// Expected daily hours, in minutes, with the activity rate applied.
  pub fn expected_minutes_per_day(&self) -> u32 {
    let base = crate::model::opt_hhmm::parse_hhmm(&self.hours_per_day)
      .map(|t| chrono::Timelike::hour(&t) * 60 + chrono::Timelike::minute(&t))
      .unwrap_or(492);
    (base as f64 * self.activity_rate).round() as u32
  }

  /// Effective Toggl token: the environment variable takes precedence over the file.
  pub fn toggl_token(&self) -> Option<String> {
    std::env::var("TOGGL_API_TOKEN").ok().filter(|t| !t.is_empty()).or_else(|| self.toggl.api_token.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn expected_minutes_follow_activity_rate() {
    let mut s = Settings::default();
    assert_eq!(s.expected_minutes_per_day(), 492); // 8h12
    s.activity_rate = 0.8;
    assert_eq!(s.expected_minutes_per_day(), 394);
    s.hours_per_day = "08:00".into();
    s.activity_rate = 1.0;
    assert_eq!(s.expected_minutes_per_day(), 480);
  }

  #[test]
  fn settings_file_is_forward_compatible() {
    // A partial, hand-written file must stay readable.
    let s: Settings = serde_json::from_str(r#"{"employee_number": 351339}"#).unwrap();
    assert_eq!(s.employee_number, 351339);
    assert_eq!(s.school_number, 1);
    assert_eq!(s.export.aggregate, Aggregate::Month);
    assert_eq!(s.export.header.len(), 9);
  }
}
