//! Rapatriement de l'historique depuis Toggl Track.
//!
//! Deux chemins d'accès : l'API v9 (`/me/time_entries`), limitée aux derniers
//! mois sur le plan gratuit, et la Reports API v3 qui pagine sur des périodes
//! plus longues. On tente la seconde en premier et on retombe sur la première.
//!
//! La correspondance Toggl → SageX passe par les tâches du catalogue : chacune
//! porte le couple (client, projet) Toggl dont elle provient.

use crate::model::{Catalog, Entry, Source};
use crate::settings::Settings;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveTime};
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};

const API: &str = "https://api.track.toggl.com/api/v9";
const REPORTS: &str = "https://api.track.toggl.com/reports/api/v3";

pub struct TogglClient {
  http: reqwest::Client,
  token: String,
}

#[derive(Debug, Deserialize)]
pub struct Workspace {
  pub id: i64,
  pub name: String,
}

#[derive(Debug, Deserialize)]
struct TogglProject {
  id: i64,
  name: String,
  client_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TogglClientRecord {
  id: i64,
  name: String,
}

/// Saisie Toggl ramenée à ce qui nous intéresse.
#[derive(Debug, Clone)]
pub struct TogglEntry {
  pub id: i64,
  pub description: String,
  pub project_id: Option<i64>,
  pub start: DateTime<FixedOffset>,
  pub seconds: i64,
}

#[derive(Debug, Deserialize)]
struct V9Entry {
  id: i64,
  #[serde(default)]
  description: Option<String>,
  #[serde(default)]
  project_id: Option<i64>,
  start: String,
  #[serde(default)]
  duration: i64,
}

#[derive(Debug, Deserialize)]
struct ReportGroup {
  #[serde(default)]
  description: Option<String>,
  #[serde(default)]
  project_id: Option<i64>,
  #[serde(default)]
  time_entries: Vec<ReportEntry>,
}

#[derive(Debug, Deserialize)]
struct ReportEntry {
  id: i64,
  seconds: i64,
  start: String,
}

impl TogglClient {
  pub fn new(token: String) -> Result<Self> {
    Ok(TogglClient {
      http: reqwest::Client::builder().user_agent("sagex-hours").build()?,
      token,
    })
  }

  async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
    let response = self
      .http
      .get(url)
      .basic_auth(&self.token, Some("api_token"))
      .send()
      .await
      .with_context(|| format!("appel de {url}"))?;
    if !response.status().is_success() {
      bail!("{url} → {} {}", response.status(), response.text().await.unwrap_or_default());
    }
    Ok(response.json().await.with_context(|| format!("réponse de {url}"))?)
  }

  pub async fn workspaces(&self) -> Result<Vec<Workspace>> {
    self.get(&format!("{API}/workspaces")).await
  }

  /// Espace de travail choisi par son nom, ou le premier disponible.
  pub async fn workspace(&self, wanted: Option<&str>) -> Result<Workspace> {
    let workspaces = self.workspaces().await?;
    if workspaces.is_empty() {
      bail!("aucun espace de travail Toggl accessible avec ce jeton");
    }
    match wanted {
      Some(name) => workspaces
        .into_iter()
        .find(|w| w.name == name)
        .with_context(|| format!("espace de travail « {name} » introuvable")),
      None => Ok(workspaces.into_iter().next().expect("liste non vide")),
    }
  }

  /// Projet Toggl → (nom du projet, nom du client).
  pub async fn project_index(&self, workspace: i64) -> Result<HashMap<i64, (String, String)>> {
    let projects: Vec<TogglProject> = self.get(&format!("{API}/workspaces/{workspace}/projects")).await?;
    let clients: Vec<TogglClientRecord> =
      self.get(&format!("{API}/workspaces/{workspace}/clients")).await.unwrap_or_default();
    let by_id: HashMap<i64, String> = clients.into_iter().map(|c| (c.id, c.name)).collect();

    Ok(
      projects
        .into_iter()
        .map(|project| {
          let client = project.client_id.and_then(|id| by_id.get(&id).cloned()).unwrap_or_default();
          (project.id, (project.name, client))
        })
        .collect(),
    )
  }

  /// Saisies de la période, Reports API d'abord puis repli sur l'API v9.
  pub async fn entries(&self, workspace: i64, from: NaiveDate, to: NaiveDate) -> Result<(Vec<TogglEntry>, &'static str)> {
    match self.entries_via_reports(workspace, from, to).await {
      Ok(entries) => Ok((entries, "reports v3")),
      Err(reports_error) => {
        tracing::warn!("Reports API indisponible ({reports_error:#}), repli sur l'API v9");
        let entries = self.entries_via_v9(from, to).await.context("repli sur l'API v9")?;
        Ok((entries, "api v9"))
      }
    }
  }

  async fn entries_via_reports(&self, workspace: i64, from: NaiveDate, to: NaiveDate) -> Result<Vec<TogglEntry>> {
    let url = format!("{REPORTS}/workspace/{workspace}/search/time_entries");
    let mut out = Vec::new();
    let mut first_row: Option<i64> = None;

    loop {
      let mut body = json!({ "start_date": from.to_string(), "end_date": to.to_string(), "page_size": 200 });
      if let Some(row) = first_row {
        body["first_row_number"] = json!(row);
      }

      let response = self.http.post(&url).basic_auth(&self.token, Some("api_token")).json(&body).send().await?;
      if !response.status().is_success() {
        bail!("{} {}", response.status(), response.text().await.unwrap_or_default());
      }
      let next = response
        .headers()
        .get("x-next-row-number")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());

      let groups: Vec<ReportGroup> = response.json().await?;
      let empty = groups.is_empty();
      for group in groups {
        for entry in group.time_entries {
          out.push(TogglEntry {
            id: entry.id,
            description: group.description.clone().unwrap_or_default(),
            project_id: group.project_id,
            start: DateTime::parse_from_rfc3339(&entry.start).with_context(|| format!("date « {} »", entry.start))?,
            seconds: entry.seconds,
          });
        }
      }

      match next {
        Some(row) if !empty => first_row = Some(row),
        _ => break,
      }
    }
    Ok(out)
  }

  async fn entries_via_v9(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<TogglEntry>> {
    // `end_date` est exclusif côté v9.
    let url = format!("{API}/me/time_entries?start_date={from}&end_date={}", to + Duration::days(1));
    let raw: Vec<V9Entry> = self.get(&url).await?;
    raw
      .into_iter()
      // Une durée négative signale un chronomètre encore en cours.
      .filter(|entry| entry.duration > 0)
      .map(|entry| {
        Ok(TogglEntry {
          id: entry.id,
          description: entry.description.unwrap_or_default(),
          project_id: entry.project_id,
          start: DateTime::parse_from_rfc3339(&entry.start).with_context(|| format!("date « {} »", entry.start))?,
          seconds: entry.duration,
        })
      })
      .collect()
  }
}

// ---------------------------------------------------------------------------
// Conversion vers les saisies
// ---------------------------------------------------------------------------

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportSummary {
  pub fetched: usize,
  pub converted: usize,
  pub duplicates: usize,
  /// Couples (client, projet) Toggl sans tâche correspondante, avec leur volume.
  pub unmapped: Vec<UnmappedGroup>,
  pub source: String,
}

#[derive(Debug, serde::Serialize)]
pub struct UnmappedGroup {
  pub client: String,
  pub project: String,
  pub entries: usize,
  pub minutes: u32,
}

/// Transforme les saisies Toggl en saisies locales.
///
/// `existing_toggl_ids` évite de réimporter deux fois la même chose.
pub fn convert(
  toggl_entries: &[TogglEntry],
  projects: &HashMap<i64, (String, String)>,
  catalog: &Catalog,
  existing_toggl_ids: &std::collections::HashSet<i64>,
) -> (Vec<Entry>, ImportSummary) {
  let mut summary = ImportSummary { fetched: toggl_entries.len(), ..Default::default() };
  let mut unmapped: BTreeMap<(String, String), (usize, u32)> = BTreeMap::new();
  let mut entries = Vec::new();

  for toggl in toggl_entries {
    if existing_toggl_ids.contains(&toggl.id) {
      summary.duplicates += 1;
      continue;
    }

    let (project_name, client_name) = toggl
      .project_id
      .and_then(|id| projects.get(&id).cloned())
      .unwrap_or_else(|| ("(sans projet)".to_string(), String::new()));

    let minutes = (toggl.seconds as f64 / 60.0).round().max(1.0) as u32;
    let Some(preset) = catalog.preset_for_toggl(&client_name, &project_name) else {
      let slot = unmapped.entry((client_name, project_name)).or_insert((0, 0));
      slot.0 += 1;
      slot.1 += minutes;
      continue;
    };

    let local = toggl.start.with_timezone(&chrono::Local).naive_local();
    let start = NaiveTime::from_hms_opt(local.time().hour_minute().0, local.time().hour_minute().1, 0)
      .unwrap_or(local.time());
    let now = Entry::now();
    entries.push(Entry {
      id: Entry::new_id(),
      date: local.date(),
      start: Some(start),
      end: Some(start + Duration::minutes(minutes as i64)),
      minutes,
      project_id: preset.project_id.clone(),
      activity_id: preset.activity_id.clone(),
      comment: if toggl.description.is_empty() { preset.label.clone() } else { toggl.description.clone() },
      source: Source::Toggl,
      toggl_id: Some(toggl.id),
      created_at: now,
      updated_at: now,
    });
  }

  summary.converted = entries.len();
  summary.unmapped = unmapped
    .into_iter()
    .map(|((client, project), (count, minutes))| UnmappedGroup { client, project, entries: count, minutes })
    .collect();
  (entries, summary)
}

/// Accès aux heures/minutes sans dépendre du trait `Timelike` chez l'appelant.
trait HourMinute {
  fn hour_minute(&self) -> (u32, u32);
}

impl HourMinute for NaiveTime {
  fn hour_minute(&self) -> (u32, u32) {
    use chrono::Timelike;
    (self.hour(), self.minute())
  }
}

/// Jeton effectif, avec un message explicite s'il manque.
pub fn token_of(settings: &Settings) -> Result<String> {
  settings
    .toggl_token()
    .context("aucun jeton Toggl : renseigne `toggl.api_token` dans settings.json ou la variable TOGGL_API_TOKEN")
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Activity, Preset, Project, TogglRef};
  use std::collections::HashSet;

  fn catalog() -> Catalog {
    Catalog {
      projects: vec![Project {
        id: "subice".into(),
        sagex_number: 129030,
        name: "SUBICE".into(),
        color: "#000".into(),
        archived: false,
        exportable: true,
        note: None,
      }],
      activities: vec![Activity { id: "ra-d".into(), sagex_number: 29, name: "Ra&D".into(), archived: false, note: None }],
      presets: vec![Preset {
        id: "subice-ra-d-subice".into(),
        label: "Ra&D Subice".into(),
        project_id: "subice".into(),
        activity_id: "ra-d".into(),
        comment: None,
        color: None,
        toggl: Some(TogglRef { client: "Subice".into(), project: "Ra&D Subice".into() }),
        archived: false,
      }],
    }
  }

  fn toggl_entry(id: i64, start: &str, seconds: i64, description: &str) -> TogglEntry {
    TogglEntry {
      id,
      description: description.to_string(),
      project_id: Some(7),
      start: DateTime::parse_from_rfc3339(start).unwrap(),
      seconds,
    }
  }

  fn projects() -> HashMap<i64, (String, String)> {
    HashMap::from([(7, ("Ra&D Subice".to_string(), "Subice".to_string())), (8, ("Inconnu".to_string(), "Autre".to_string()))])
  }

  #[test]
  fn mapped_entries_keep_their_schedule() {
    let toggl = vec![toggl_entry(1, "2026-07-02T08:30:00+02:00", 5400, "Bibliographie")];
    let (entries, summary) = convert(&toggl, &projects(), &catalog(), &HashSet::new());

    assert_eq!(summary.converted, 1);
    let entry = &entries[0];
    assert_eq!(entry.project_id, "subice");
    assert_eq!(entry.activity_id, "ra-d");
    assert_eq!(entry.minutes, 90);
    assert_eq!(entry.comment, "Bibliographie");
    assert_eq!(entry.toggl_id, Some(1));
    assert_eq!(entry.source, Source::Toggl);
    // L'heure est convertie dans le fuseau local et la fin déduite.
    assert_eq!(entry.end.unwrap() - entry.start.unwrap(), Duration::minutes(90));
  }

  #[test]
  fn already_imported_entries_are_not_duplicated() {
    let toggl = vec![toggl_entry(1, "2026-07-02T08:30:00+02:00", 3600, "A"), toggl_entry(2, "2026-07-03T08:30:00+02:00", 3600, "B")];
    let (entries, summary) = convert(&toggl, &projects(), &catalog(), &HashSet::from([1]));
    assert_eq!(summary.duplicates, 1);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].toggl_id, Some(2));
  }

  #[test]
  fn unmapped_projects_are_reported_not_dropped_silently() {
    let mut orphan = toggl_entry(3, "2026-07-04T09:00:00+02:00", 7200, "X");
    orphan.project_id = Some(8);
    let (entries, summary) = convert(&[orphan], &projects(), &catalog(), &HashSet::new());

    assert!(entries.is_empty());
    assert_eq!(summary.unmapped.len(), 1);
    assert_eq!(summary.unmapped[0].project, "Inconnu");
    assert_eq!(summary.unmapped[0].minutes, 120);
  }

  #[test]
  fn empty_description_falls_back_to_the_task_label() {
    let (entries, _) = convert(&[toggl_entry(4, "2026-07-05T09:00:00+02:00", 1800, "")], &projects(), &catalog(), &HashSet::new());
    assert_eq!(entries[0].comment, "Ra&D Subice");
  }
}

// ---------------------------------------------------------------------------
// Import complet
// ---------------------------------------------------------------------------

/// Récupère la période chez Toggl et l'enregistre (sauf en `dry_run`).
pub async fn run(store: &crate::store::Store, from: NaiveDate, to: NaiveDate, dry_run: bool) -> Result<ImportSummary> {
  let settings = store.settings()?;
  let client = TogglClient::new(token_of(&settings)?)?;
  let workspace = client.workspace(settings.toggl.workspace.as_deref()).await?;
  let projects = client.project_index(workspace.id).await?;
  let (toggl_entries, source) = client.entries(workspace.id, from, to).await?;

  // Les identifiants déjà présents rendent l'import rejouable sans doublon.
  let mut known = std::collections::HashSet::new();
  for month in crate::model::YearMonth::range(from, to) {
    for entry in store.month(month)?.entries {
      if let Some(id) = entry.toggl_id {
        known.insert(id);
      }
    }
  }

  let catalog = store.catalog()?;
  let (entries, mut summary) = convert(&toggl_entries, &projects, &catalog, &known);
  summary.source = format!("{source} · espace « {} »", workspace.name);

  if !dry_run && !entries.is_empty() {
    store.insert_entries(entries)?;
  }
  Ok(summary)
}

impl ImportSummary {
  pub fn print(&self, dry_run: bool) {
    println!("Toggl ({}) : {} saisie(s) récupérée(s).", self.source, self.fetched);
    println!("  · {} converties{}", self.converted, if dry_run { " (rien écrit : --dry-run)" } else { "" });
    if self.duplicates > 0 {
      println!("  · {} déjà importées, ignorées", self.duplicates);
    }
    if !self.unmapped.is_empty() {
      let total: u32 = self.unmapped.iter().map(|g| g.minutes).sum();
      println!("  ! {} couple(s) client/projet sans tâche correspondante ({} au total) :", self.unmapped.len(), crate::export::format_hours(total));
      for group in &self.unmapped {
        println!(
          "      {} / {} → {} saisie(s), {}",
          if group.client.is_empty() { "(sans client)" } else { &group.client },
          group.project,
          group.entries,
          crate::export::format_hours(group.minutes)
        );
      }
      println!("      Crée les tâches manquantes dans la fenêtre « Gérer », puis relance l'import.");
    }
  }
}
