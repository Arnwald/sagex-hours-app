//! `settings.json` : identité du collaborateur, règles de calcul, options d'export.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
  /// « N° Interne Collaborateur » (SageX → Paramètres).
  pub employee_number: u32,
  /// « I_PERSONNE » (SageX → Paramètres).
  pub person_number: u64,
  /// N° école, 1 pour la HES-SO Valais.
  pub school_number: u32,
  /// Taux d'activité, 1.0 = 100 %.
  pub activity_rate: f64,
  /// Imputation moyenne pour un 100 %, `"HH:MM"`. 8h12 selon la directive 2026.
  pub hours_per_day: String,
  /// Jours fériés supplémentaires ou de remplacement (les fériés valaisans sont calculés).
  #[serde(default)]
  pub extra_holidays: Vec<NaiveDate>,
  /// Désactive le calcul automatique des fériés valaisans.
  pub compute_holidays: bool,
  pub export: ExportSettings,
  pub toggl: TogglSettings,
  pub ui: UiSettings,
  /// Date de la dernière sauvegarde complète téléchargée.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub last_backup: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportSettings {
  pub aggregate: Aggregate,
  /// Mise en forme de la colonne « Commentaire » lors d'une agrégation.
  pub comment_style: CommentStyle,
  /// Format des dates préfixant chaque commentaire agrégé.
  pub comment_date_format: String,
  pub comment_separator: String,
  /// La colonne « Commentaire » de SageX est un varchar1000.
  pub comment_max_len: usize,
  pub header: Vec<String>,
  /// Nom du fichier produit, `{from}` et `{to}` sont remplacés.
  pub file_name: String,
}

/// Comment condenser les descriptions regroupées sur une même ligne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentStyle {
  /// Une mention par saisie : `« 30.07.2026 - Daily | 29.07.2026 - Daily »`.
  Detailed,
  /// Descriptions identiques regroupées : `« Daily (2× : 30.07, 29.07) »`.
  Compact,
  /// Détaillé, et compacté seulement si la limite varchar1000 est dépassée.
  Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregate {
  /// Une ligne par (projet, activité) et par mois, datée du dernier jour du mois.
  Month,
  /// Une ligne par (projet, activité) et par jour.
  Day,
  /// Une ligne par saisie.
  None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TogglSettings {
  /// Peut aussi être fourni par la variable d'environnement `TOGGL_API_TOKEN`.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub api_token: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub workspace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
  /// Première heure affichée dans la grille de la semaine.
  pub day_start_hour: u32,
  pub day_end_hour: u32,
  /// Pas de la grille, en minutes.
  pub slot_minutes: u32,
  /// Durée par défaut d'une entrée créée en un clic.
  pub default_minutes: u32,
  /// Rappel de sauvegarde après N jours sans backup.
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
      // Ces libellés sont ceux attendus par l'importeur SageX : ne pas traduire.
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
  /// Imputation quotidienne attendue, en minutes, taux d'activité appliqué.
  pub fn expected_minutes_per_day(&self) -> u32 {
    let base = crate::model::opt_hhmm::parse_hhmm(&self.hours_per_day)
      .map(|t| chrono::Timelike::hour(&t) * 60 + chrono::Timelike::minute(&t))
      .unwrap_or(492);
    (base as f64 * self.activity_rate).round() as u32
  }

  /// Token Toggl effectif : variable d'environnement prioritaire sur le fichier.
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
    // Un fichier partiel écrit à la main doit rester lisible.
    let s: Settings = serde_json::from_str(r#"{"employee_number": 351339}"#).unwrap();
    assert_eq!(s.employee_number, 351339);
    assert_eq!(s.school_number, 1);
    assert_eq!(s.export.aggregate, Aggregate::Month);
    assert_eq!(s.export.header.len(), 9);
  }
}
