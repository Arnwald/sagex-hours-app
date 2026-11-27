//! The three lists: SageX projects, SageX activities, tasks.

use crate::api::{ApiError, ApiResult};
use crate::app::Space;
use crate::model::{Activity, Catalog, Preset, Project};
use crate::parse;
use crate::slug;
use crate::store::Change;
use axum::extract::{Path, Query};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

pub async fn get_catalog(space: Space) -> ApiResult<Json<Catalog>> {
  Ok(Json(space.store.catalog()?))
}

#[derive(Debug, Deserialize)]
pub struct ItemInput {
  #[serde(default)]
  pub id: Option<String>,
  #[serde(default)]
  pub name: Option<String>,
  /// Tasks use `label`, projects and activities `name`.
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

pub async fn create_item(space: Space, Path(kind): Path<String>, Json(input): Json<ItemInput>) -> ApiResult<Json<Value>> {
  let _guard = space.write_lock.lock().await;
  let mut catalog = space.store.catalog()?;

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
      let project = input.project.as_ref().ok_or_else(|| ApiError::bad_request(tr!("`{field}` missing", field = "project")))?;
      let activity = input.activity.as_ref().ok_or_else(|| ApiError::bad_request(tr!("`{field}` missing", field = "activity")))?;
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

  save(&space, &catalog)?;
  Ok(Json(created))
}

pub async fn update_item(
  space: Space,
  Path((kind, id)): Path<(String, String)>,
  Json(input): Json<ItemInput>,
) -> ApiResult<Json<Value>> {
  let _guard = space.write_lock.lock().await;
  let mut catalog = space.store.catalog()?;

  // References are resolved before the target list is borrowed mutably.
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

  save(&space, &catalog)?;
  Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
  /// Deletes even if entries use the item (they become orphans).
  #[serde(default)]
  pub force: Option<bool>,
}

pub async fn delete_item(
  space: Space,
  Path((kind, id)): Path<(String, String)>,
  Query(query): Query<DeleteQuery>,
) -> ApiResult<Json<Value>> {
  let _guard = space.write_lock.lock().await;
  let mut catalog = space.store.catalog()?;

  // A project or an activity still in use does not vanish silently.
  if kind != "presets" && !query.force.unwrap_or(false) {
    let used = count_usages(&space, &kind, &id)?;
    if used > 0 {
      return Err(ApiError::conflict(tr!(
        "{count} entries still use “{id}” — archive it, or retry with ?force=true",
        count = used,
        id = id
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

  save(&space, &catalog)?;
  Ok(Json(json!({ "deleted": id })))
}

// ---------------------------------------------------------------------------

fn save(space: &Space, catalog: &Catalog) -> ApiResult<()> {
  space.store.save_catalog(catalog)?;
  space.mark_self_write();
  space.store.notify(Change::scope("catalog"));
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

fn count_usages(space: &Space, kind: &str, id: &str) -> ApiResult<usize> {
  let mut count = 0;
  for month in space.store.months()? {
    for entry in space.store.month(month)?.entries {
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
    None => Err(ApiError::bad_request(tr!("`{field}` missing", field = field))),
  }
}

fn new_id(explicit: Option<&str>, title: &str, mut taken: HashSet<String>, hint: Option<u32>) -> String {
  let base = explicit.map(slug::slugify).unwrap_or_else(|| slug::slugify(title));
  slug::unique(&base, &mut taken, hint)
}

fn taken(ids: impl Iterator<Item = String>) -> HashSet<String> {
  ids.collect()
}

/// Two items with the same SageX number are almost always a mistake.
fn warn_duplicate_number<'a>(existing: impl Iterator<Item = (u32, &'a str)>, number: u32) -> ApiResult<()> {
  if number == 0 {
    return Ok(());
  }
  for (other_number, name) in existing {
    if other_number == number {
      return Err(ApiError::conflict(tr!("SageX number {number} is already used by “{name}”", number = number, name = name)));
    }
  }
  Ok(())
}

fn missing(kind: &str, id: &str) -> ApiError {
  ApiError::not_found(tr!("no item “{id}” in {kind}", id = id, kind = kind))
}

fn unknown_kind(kind: &str) -> ApiError {
  ApiError::bad_request(tr!("unknown list “{kind}” — use projects, activities or presets", kind = kind))
}
