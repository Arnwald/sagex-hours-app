//! Conversion du `sagex-rs.ron` (Toggl → SageX) vers le catalogue à trois listes.
//!
//! Dans le RON, la paire (n° activité, nom activité) est répétée dans chaque
//! projet Toggl. On la dédoublonne ici pour obtenir une liste d'activités
//! réutilisable par n'importe quel projet, conformément à ce que SageX attend
//! réellement (cf. directive « Saisie des heures SageX 2026 »).

use crate::model::{Activity, Catalog, Preset, Project, TogglRef};
use crate::settings::{Aggregate, Settings};
use crate::slug;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Sous-ensemble utile du `sagex-rs.ron`. Les champs absents sont ignorés.
#[derive(Debug, Deserialize)]
#[serde(rename = "Config")]
struct RonConfig {
  #[serde(default)]
  aggregate_by_month: Option<bool>,
  #[serde(default)]
  toggl_workspace: Option<String>,
  #[serde(default)]
  toggl_api_token: Option<String>,
  #[serde(default)]
  sagex_employee_number: u32,
  #[serde(default)]
  sagex_person_number: u64,
  #[serde(default = "one")]
  sagex_school_number: u32,
  #[serde(default)]
  sagex_activities: HashMap<String, u32>,
  /// client Toggl → projet Toggl → ((n° projet SageX, nom), (n° activité, nom))
  #[serde(default)]
  projects: HashMap<String, HashMap<String, ((u32, String), (u32, String))>>,
  #[serde(default)]
  toggl_projects_to_ignore: Vec<String>,
}

fn one() -> u32 {
  1
}

/// Identifiants lisibles pour les activités SageX standard (numéros stables).
fn preferred_activity_id(number: u32) -> Option<&'static str> {
  Some(match number {
    0 => "non-sagex",
    3 => "absence",
    4 => "seance",
    5 => "cours",
    12 => "mandats",
    14 => "admin",
    29 => "ra-d",
    45 => "ens-specifique",
    279 => "acquisition",
    20103 => "formation",
    24798 => "taches-liees",
    _ => return None,
  })
}

#[derive(Debug, Default)]
pub struct ImportReport {
  pub projects: usize,
  pub activities: usize,
  pub presets: usize,
  pub warnings: Vec<String>,
  pub notes: Vec<String>,
}

impl ImportReport {
  pub fn print(&self) {
    println!("Catalogue construit : {} projets, {} activités, {} tâches.", self.projects, self.activities, self.presets);
    for note in &self.notes {
      println!("  · {note}");
    }
    for warning in &self.warnings {
      println!("  ! {warning}");
    }
  }
}

/// Convertit un `sagex-rs.ron` en catalogue, et complète les réglages.
pub fn convert(ron_source: &str, settings: &mut Settings) -> Result<(Catalog, ImportReport)> {
  let config: RonConfig = ron::from_str(ron_source).context("lecture du sagex-rs.ron")?;
  let mut report = ImportReport::default();

  // -- réglages issus du RON ------------------------------------------------
  if config.sagex_employee_number != 0 {
    settings.employee_number = config.sagex_employee_number;
  }
  if config.sagex_person_number != 0 {
    settings.person_number = config.sagex_person_number;
  }
  settings.school_number = config.sagex_school_number;
  settings.export.aggregate = if config.aggregate_by_month.unwrap_or(false) { Aggregate::Month } else { Aggregate::Day };
  if settings.toggl.api_token.is_none() {
    settings.toggl.api_token = config.toggl_api_token.clone();
  }
  if settings.toggl.workspace.is_none() {
    settings.toggl.workspace = config.toggl_workspace.clone();
  }

  // -- activités ------------------------------------------------------------
  // On part de `sagex_activities` (la liste de référence), puis on ajoute
  // celles que seuls les projets mentionnent.
  let mut activity_names: BTreeMap<u32, String> = BTreeMap::new();
  for (name, number) in &config.sagex_activities {
    activity_names.insert(*number, name.clone());
  }
  // Ordre déterministe : on trie les clients et projets avant de parcourir.
  let clients: BTreeMap<&String, BTreeMap<&String, &((u32, String), (u32, String))>> = config
    .projects
    .iter()
    .map(|(client, projects)| (client, projects.iter().collect()))
    .collect();

  for (client, projects) in &clients {
    for (project, ((project_number, project_name), (activity_number, activity_name))) in projects {
      match activity_names.get(activity_number) {
        None => {
          activity_names.insert(*activity_number, activity_name.clone());
        }
        Some(known) if known != activity_name => {
          report.warnings.push(format!(
            "activité {activity_number} nommée « {known} » dans sagex_activities et « {activity_name} » pour {client}/{project} — « {known} » retenu"
          ));
        }
        Some(_) => {}
      }
      let _ = (project_number, project_name);
    }
  }

  let mut activity_ids: HashSet<String> = HashSet::new();
  let mut activity_by_number: HashMap<u32, String> = HashMap::new();
  let mut activities = Vec::new();
  for (number, name) in &activity_names {
    let base = preferred_activity_id(*number).map(|s| s.to_string()).unwrap_or_else(|| slug::slugify(name));
    let id = slug::unique(&base, &mut activity_ids, Some(*number));
    activity_by_number.insert(*number, id.clone());
    activities.push(Activity {
      id,
      sagex_number: *number,
      name: name.clone(),
      archived: false,
      note: None,
    });
  }

  // -- projets --------------------------------------------------------------
  let mut project_names: BTreeMap<u32, String> = BTreeMap::new();
  for (client, projects) in &clients {
    for (project, ((number, name), _)) in projects {
      match project_names.get(number) {
        Some(known) if known != name => report.warnings.push(format!(
          "projet {number} nommé « {known} » et « {name} » (pour {client}/{project}) — « {known} » retenu"
        )),
        _ => {
          project_names.entry(*number).or_insert_with(|| name.clone());
        }
      }
    }
  }

  let mut project_ids: HashSet<String> = HashSet::new();
  let mut project_by_number: HashMap<u32, String> = HashMap::new();
  let mut projects = Vec::new();
  for (index, (number, name)) in project_names.iter().enumerate() {
    // Le numéro 0 est la convention du RON pour « ne pas envoyer dans SageX ».
    let (base, name, exportable, note) = if *number == 0 {
      (
        "non-sagex".to_string(),
        "Non SageX (interne)".to_string(),
        false,
        Some("Suivi interne uniquement : exclu de l'export SageX.".to_string()),
      )
    } else {
      (slug::slugify(name), name.clone(), true, None)
    };
    let id = slug::unique(&base, &mut project_ids, Some(*number));
    project_by_number.insert(*number, id.clone());
    projects.push(Project {
      id,
      sagex_number: *number,
      name,
      color: palette(index),
      archived: false,
      exportable,
      note,
    });
  }

  if let Some(suspicious) = projects.iter().find(|p| p.exportable && p.name.to_lowercase().contains("non sagex")) {
    report.warnings.push(format!(
      "le projet {} est nommé « {} » mais son numéro n'est pas nul : il SERA exporté (la directive 2026 y place les absences longue durée) — renomme-le dans la fenêtre de gestion",
      suspicious.sagex_number, suspicious.name
    ));
  }

  // -- tâches (presets) -----------------------------------------------------
  // Un libellé de projet Toggl peut exister chez plusieurs clients : on ne
  // préfixe par le client que dans ce cas.
  let mut label_counts: HashMap<&String, usize> = HashMap::new();
  for projects in clients.values() {
    for project in projects.keys() {
      *label_counts.entry(*project).or_default() += 1;
    }
  }

  let mut preset_ids: HashSet<String> = HashSet::new();
  let mut presets = Vec::new();
  for (client, projects) in &clients {
    for (project, ((project_number, _), (activity_number, _))) in projects {
      let id = slug::unique(&format!("{}-{}", slug::slugify(client), slug::slugify(project)), &mut preset_ids, None);
      let label = if label_counts.get(*project).copied().unwrap_or(0) > 1 {
        format!("{client} · {project}")
      } else {
        (*project).clone()
      };
      presets.push(Preset {
        id,
        label,
        project_id: project_by_number[project_number].clone(),
        activity_id: activity_by_number[activity_number].clone(),
        comment: None,
        color: None,
        toggl: Some(TogglRef { client: (*client).clone(), project: (*project).clone() }),
        archived: false,
      });
    }
  }
  presets.sort_by(|a, b| a.label.cmp(&b.label));

  if !config.toggl_projects_to_ignore.is_empty() {
    report.notes.push(format!(
      "projets Toggl auparavant ignorés : {} — ils sont désormais rattachés au projet « Non SageX (interne) », donc visibles dans l'app et exclus de l'export",
      config.toggl_projects_to_ignore.join(", ")
    ));
  }

  report.projects = projects.len();
  report.activities = activities.len();
  report.presets = presets.len();
  Ok((Catalog { projects, activities, presets }, report))
}

/// Palette lisible en clair comme en sombre.
fn palette(index: usize) -> String {
  const COLORS: &[&str] = &[
    "#4c78a8", "#f58518", "#54a24b", "#b279a2", "#e45756", "#72b7b2", "#eeca3b", "#9d755d", "#7f7fb3", "#bab0ac",
  ];
  COLORS[index % COLORS.len()].to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE: &str = r#"Config(
    import: "api",
    out_path: "/tmp",
    aggregate_by_month: Some(true),
    in_path: None,
    toggl_workspace: Some("Workspace"),
    toggl_api_token: Some("secret"),
    sagex_employee_number: 351339,
    sagex_person_number: 9008376769,
    sagex_school_number: 1,
    sagex_header: ["a"],
    date_fmt: "%d/%m/%Y",
    hour_fmt: "{:02}h{:02}",
    sagex_activities: {
        "Séance de travail": 4,
        "Tâches administratives générales": 14,
        "Réalisation de projets ou d'activités Ra&D": 29,
    },
    projects: {
        "Constellium": {
            "Admin": ((89403, "SPL Base"), (14, "Tâches administratives générales")),
            "Daily": ((89403, "SPL Base"), (4, "Séance de travail")),
        },
        "HESSO": {
            "Admin": ((89403, "SPL Base"), (14, "Tâches administratives générales")),
            "Holiday": ((0, "Non SageX Project"), (0, "Non SageX Project")),
            "Sick": ((142259, "Non SageX Project"), (3, "Absence")),
        },
        "Subice": {
            "Ra&D Subice": ((129030, "SUBICE (MARVIS) FNS/SEFRI"), (29, "Réalisation de projets ou d'activités Ra&D")),
        },
    },
    toggl_projects_to_ignore: ["Compensation"],
)"#;

  /// Petite aide de lecture pour les assertions ci-dessous.
  fn by_number<'a, T>(items: &'a [T], number: u32, field: impl Fn(&'a T) -> &'a String) -> &'a str
  where
    T: HasNumber,
  {
    field(items.iter().find(|item| item.number() == number).expect("élément présent"))
  }

  trait HasNumber {
    fn number(&self) -> u32;
  }
  impl HasNumber for Project {
    fn number(&self) -> u32 {
      self.sagex_number
    }
  }
  impl HasNumber for Activity {
    fn number(&self) -> u32 {
      self.sagex_number
    }
  }

  fn convert_sample() -> (Catalog, ImportReport, Settings) {
    let mut settings = Settings::default();
    let (catalog, report) = convert(SAMPLE, &mut settings).unwrap();
    (catalog, report, settings)
  }

  #[test]
  fn settings_are_seeded_from_the_ron() {
    let (_, _, settings) = convert_sample();
    assert_eq!(settings.employee_number, 351339);
    assert_eq!(settings.person_number, 9008376769);
    assert_eq!(settings.export.aggregate, Aggregate::Month);
    assert_eq!(settings.toggl.workspace.as_deref(), Some("Workspace"));
  }

  #[test]
  fn activities_are_deduplicated_into_a_single_list() {
    let (catalog, _, _) = convert_sample();
    // 4 activités du RON + « Absence » (3) et le 0 rencontrés dans les projets.
    let numbers: Vec<u32> = catalog.activities.iter().map(|a| a.sagex_number).collect();
    assert_eq!(numbers, vec![0, 3, 4, 14, 29]);
    assert_eq!(by_number(&catalog.activities, 29, |a| &a.id), "ra-d");
    assert_eq!(by_number(&catalog.activities, 4, |a| &a.id), "seance");
    // Chaque activité n'apparaît qu'une fois, quel que soit le nombre de projets.
    assert_eq!(catalog.activities.iter().filter(|a| a.sagex_number == 14).count(), 1);
  }

  #[test]
  fn project_zero_becomes_a_non_exportable_project() {
    let (catalog, _, _) = convert_sample();
    let non_sagex = catalog.projects.iter().find(|p| p.sagex_number == 0).unwrap();
    assert_eq!(non_sagex.id, "non-sagex");
    assert!(!non_sagex.exportable);
    // 142259 garde son numéro et reste exportable.
    let absences = catalog.projects.iter().find(|p| p.sagex_number == 142259).unwrap();
    assert!(absences.exportable);
  }

  #[test]
  fn duplicate_toggl_labels_are_prefixed_by_client() {
    let (catalog, _, _) = convert_sample();
    let labels: Vec<&str> = catalog.presets.iter().map(|p| p.label.as_str()).collect();
    assert!(labels.contains(&"Constellium · Admin"), "{labels:?}");
    assert!(labels.contains(&"HESSO · Admin"), "{labels:?}");
    assert!(labels.contains(&"Daily"), "{labels:?}");

    let preset = catalog.preset_for_toggl("Subice", "Ra&D Subice").unwrap();
    assert_eq!(preset.project_id, by_number(&catalog.projects, 129030, |p| &p.id));
    assert_eq!(preset.activity_id, "ra-d");
    assert_eq!(preset.id, "subice-ra-d-subice");
  }

  #[test]
  fn misnamed_exported_project_is_reported() {
    let (_, report, _) = convert_sample();
    assert!(
      report.warnings.iter().any(|w| w.contains("142259") && w.contains("SERA exporté")),
      "{:?}",
      report.warnings
    );
  }
}
