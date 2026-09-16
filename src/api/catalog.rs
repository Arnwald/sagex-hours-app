//! Gestion des trois listes : projets SageX, activités SageX, tâches.

use crate::api::{ApiError, ApiResult};
use crate::app::AppState;
use crate::model::{Activity, Catalog, Preset, Project};
use crate::parse;
use crate::slug;
use crate::store::Change;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

pub async fn get_catalog(State(state): State<AppState>) -> ApiResult<Json<Catalog>> {
  Ok(Json(state.store.catalog()?))
}

#[derive(Debug, Deserialize)]
pub struct ItemInput {
  #[serde(default)]
  pub id: Option<String>,
  #[serde(default)]
  pub name: Option<String>,
  /// Les tâches utilisent `label`, les projets et activités `name`.
  #[serde(default)]
  pub label: Option<String>,
  #[serde(default, alias = "number")]
  pub sagex_number: Option<u32>,
  #[serde(default)]
  pub color: Option<String>,
  #[serde(default)]
  pub archived: Option<bool>,
  #[serde(default)]
  pub exportable: Option<bool>,
  #[serde(default)]
  pub note: Option<String>,
  #[serde(default, alias = "project_id")]
  pub project: Option<parse::Loose>,
  #[serde(default, alias = "activity_id")]
  pub activity: Option<parse::Loose>,
  #[serde(default)]
  pub comment: Option<String>,
  #[serde(default)]
  pub toggl: Option<crate::model::TogglRef>,
}

impl ItemInput {
  fn title(&self) -> Option<&str> {
    self.label.as_deref().or(self.name.as_deref())
  }
}

pub async fn create_item(
  State(state): State<AppState>,
  Path(kind): Path<String>,
  Json(input): Json<ItemInput>,
) -> ApiResult<Json<Value>> {
  let _guard = state.write_lock.lock().await;
  let mut catalog = state.store.catalog()?;

  let created = match kind.as_str() {
    "projects" => {
      let name = require_title(&input, "name")?;
      let number = input.sagex_number.unwrap_or(0);
      warn_duplicate_number(catalog.projects.iter().map(|p| (p.sagex_number, p.name.as_str())), number)?;
      let id = new_id(input.id.as_deref(), &name, taken(catalog.projects.iter().map(|p| p.id.clone())), Some(number));
      let project = Project {
        id,
        sagex_number: number,
        name,
        color: input.color.unwrap_or_else(|| "#6b7a8f".to_string()),
        archived: input.archived.unwrap_or(false),
        exportable: input.exportable.unwrap_or(number != 0),
        note: input.note,
      };
      let value = serde_json::to_value(&project).map_err(anyhow::Error::from)?;
      catalog.projects.push(project);
      value
    }
    "activities" => {
      let name = require_title(&input, "name")?;
      let number = input.sagex_number.unwrap_or(0);
      warn_duplicate_number(catalog.activities.iter().map(|a| (a.sagex_number, a.name.as_str())), number)?;
      let id = new_id(input.id.as_deref(), &name, taken(catalog.activities.iter().map(|a| a.id.clone())), Some(number));
      let activity = Activity { id, sagex_number: number, name, archived: input.archived.unwrap_or(false), note: input.note };
      let value = serde_json::to_value(&activity).map_err(anyhow::Error::from)?;
      catalog.activities.push(activity);
      value
    }
    "presets" => {
      let label = require_title(&input, "label")?;
      let project = input.project.as_ref().ok_or_else(|| ApiError::bad_request("`project` manquant"))?;
      let activity = input.activity.as_ref().ok_or_else(|| ApiError::bad_request("`activity` manquant"))?;
      let project_id = parse::find_project(&catalog, &project.as_text()).into_result().map_err(ApiError::bad_request)?.id.clone();
      let activity_id =
        parse::find_activity(&catalog, &activity.as_text()).into_result().map_err(ApiError::bad_request)?.id.clone();
      let id = new_id(input.id.as_deref(), &label, taken(catalog.presets.iter().map(|p| p.id.clone())), None);
      let preset = Preset {
        id,
        label,
        project_id,
        activity_id,
        comment: input.comment,
        color: input.color,
        toggl: input.toggl,
        archived: input.archived.unwrap_or(false),
      };
      let value = serde_json::to_value(&preset).map_err(anyhow::Error::from)?;
      catalog.presets.push(preset);
      value
    }
    other => return Err(unknown_kind(other)),
  };

  save(&state, &catalog)?;
  Ok(Json(created))
}

pub async fn update_item(
  State(state): State<AppState>,
  Path((kind, id)): Path<(String, String)>,
  Json(input): Json<ItemInput>,
) -> ApiResult<Json<Value>> {
  let _guard = state.write_lock.lock().await;
  let mut catalog = state.store.catalog()?;

  // Les références sont résolues avant l'emprunt mutable de la liste visée.
  let project_id = match &input.project {
    Some(reference) => Some(parse::find_project(&catalog, &reference.as_text()).into_result().map_err(ApiError::bad_request)?.id.clone()),
    None => None,
  };
  let activity_id = match &input.activity {
    Some(reference) => Some(parse::find_activity(&catalog, &reference.as_text()).into_result().map_err(ApiError::bad_request)?.id.clone()),
    None => None,
  };

  let updated = match kind.as_str() {
    "projects" => {
      let project = catalog.projects.iter_mut().find(|p| p.id == id).ok_or_else(|| missing(&kind, &id))?;
      if let Some(name) = input.title() {
        project.name = name.to_string();
      }
      if let Some(number) = input.sagex_number {
        project.sagex_number = number;
      }
      if let Some(color) = input.color {
        project.color = color;
      }
      if let Some(archived) = input.archived {
        project.archived = archived;
      }
      if let Some(exportable) = input.exportable {
        project.exportable = exportable;
      }
      if let Some(note) = input.note {
        project.note = Some(note).filter(|n| !n.is_empty());
      }
      serde_json::to_value(&*project).map_err(anyhow::Error::from)?
    }
    "activities" => {
      let activity = catalog.activities.iter_mut().find(|a| a.id == id).ok_or_else(|| missing(&kind, &id))?;
      if let Some(name) = input.title() {
        activity.name = name.to_string();
      }
      if let Some(number) = input.sagex_number {
        activity.sagex_number = number;
      }
      if let Some(archived) = input.archived {
        activity.archived = archived;
      }
      if let Some(note) = input.note {
        activity.note = Some(note).filter(|n| !n.is_empty());
      }
      serde_json::to_value(&*activity).map_err(anyhow::Error::from)?
    }
    "presets" => {
      let preset = catalog.presets.iter_mut().find(|p| p.id == id).ok_or_else(|| missing(&kind, &id))?;
      if let Some(label) = input.title() {
        preset.label = label.to_string();
      }
      if let Some(project_id) = project_id {
        preset.project_id = project_id;
      }
      if let Some(activity_id) = activity_id {
        preset.activity_id = activity_id;
      }
      if let Some(comment) = input.comment {
        preset.comment = Some(comment).filter(|c| !c.is_empty());
      }
      if let Some(color) = input.color {
        preset.color = Some(color).filter(|c| !c.is_empty());
      }
      if let Some(archived) = input.archived {
        preset.archived = archived;
      }
      if input.toggl.is_some() {
        preset.toggl = input.toggl;
      }
      serde_json::to_value(&*preset).map_err(anyhow::Error::from)?
    }
    other => return Err(unknown_kind(other)),
  };

  save(&state, &catalog)?;
  Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
  /// Supprime même si des saisies utilisent l'élément (elles deviennent orphelines).
  #[serde(default)]
  pub force: Option<bool>,
}

pub async fn delete_item(
  State(state): State<AppState>,
  Path((kind, id)): Path<(String, String)>,
  Query(query): Query<DeleteQuery>,
) -> ApiResult<Json<Value>> {
  let _guard = state.write_lock.lock().await;
  let mut catalog = state.store.catalog()?;

  // Un projet ou une activité encore utilisé ne disparaît pas en silence.
  if kind != "presets" && !query.force.unwrap_or(false) {
    let used = count_usages(&state, &kind, &id)?;
    if used > 0 {
      return Err(ApiError::conflict(format!(
        "{used} saisie(s) utilisent encore « {id} » — archive-le, ou relance avec ?force=true"
      )));
    }
  }

  let removed = match kind.as_str() {
    "projects" => remove_by_id(&mut catalog.projects, &id, |p| &p.id),
    "activities" => remove_by_id(&mut catalog.activities, &id, |a| &a.id),
    "presets" => remove_by_id(&mut catalog.presets, &id, |p| &p.id),
    other => return Err(unknown_kind(other)),
  };
  if !removed {
    return Err(missing(&kind, &id));
  }

  save(&state, &catalog)?;
  Ok(Json(json!({ "deleted": id })))
}

// ---------------------------------------------------------------------------

fn save(state: &AppState, catalog: &Catalog) -> ApiResult<()> {
  state.store.save_catalog(catalog)?;
  state.mark_self_write();
  state.store.notify(Change::scope("catalog"));
  Ok(())
}

fn remove_by_id<T>(items: &mut Vec<T>, id: &str, id_of: impl Fn(&T) -> &str) -> bool {
  match items.iter().position(|i| id_of(i) == id) {
    Some(index) => {
      items.remove(index);
      true
    }
    None => false,
  }
}

fn count_usages(state: &AppState, kind: &str, id: &str) -> ApiResult<usize> {
  let mut count = 0;
  for month in state.store.months()? {
    for entry in state.store.month(month)?.entries {
      let hit = match kind {
        "projects" => entry.project_id == id,
        "activities" => entry.activity_id == id,
        _ => false,
      };
      if hit {
        count += 1;
      }
    }
  }
  Ok(count)
}

fn require_title(input: &ItemInput, field: &str) -> ApiResult<String> {
  match input.title().map(str::trim).filter(|t| !t.is_empty()) {
    Some(title) => Ok(title.to_string()),
    None => Err(ApiError::bad_request(format!("`{field}` manquant"))),
  }
}

fn new_id(explicit: Option<&str>, title: &str, mut taken: HashSet<String>, hint: Option<u32>) -> String {
  let base = explicit.map(slug::slugify).unwrap_or_else(|| slug::slugify(title));
  slug::unique(&base, &mut taken, hint)
}

fn taken(ids: impl Iterator<Item = String>) -> HashSet<String> {
  ids.collect()
}

/// Deux éléments portant le même numéro SageX sont presque toujours une erreur.
fn warn_duplicate_number<'a>(existing: impl Iterator<Item = (u32, &'a str)>, number: u32) -> ApiResult<()> {
  if number == 0 {
    return Ok(());
  }
  for (other_number, name) in existing {
    if other_number == number {
      return Err(ApiError::conflict(format!("le numéro SageX {number} est déjà utilisé par « {name} »")));
    }
  }
  Ok(())
}

fn missing(kind: &str, id: &str) -> ApiError {
  ApiError::not_found(format!("aucun élément « {id} » dans {kind}"))
}

fn unknown_kind(kind: &str) -> ApiError {
  ApiError::bad_request(format!("liste « {kind} » inconnue — utilise projects, activities ou presets"))
}
