//! Saisies : lecture, création (unitaire ou en masse), modification,
//! duplication de jours ou de semaines, chronomètre.

use crate::api::{ApiError, ApiResult};
use crate::app::AppState;
use crate::model::{opt_hhmm, Entry, RunningTimer, Source, YearMonth};
use crate::parse::{self, Loose};
use crate::settings::Settings;
use crate::store::Change;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Weekday};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Entrées de requête
// ---------------------------------------------------------------------------

/// Description d'une saisie telle qu'on peut l'écrire à la main.
///
/// Les références acceptent l'identifiant, le numéro SageX ou le nom ; la durée
/// accepte `1h30`, `90`, `1.5h`… Les alias couvrent les formulations naturelles
/// (`project_id`, `description`, `remarque`).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryInput {
  #[serde(default)]
  pub id: Option<String>,
  #[serde(default)]
  pub date: Option<String>,
  #[serde(default, deserialize_with = "double_option")]
  pub start: Option<Option<String>>,
  #[serde(default, deserialize_with = "double_option")]
  pub end: Option<Option<String>>,
  #[serde(default)]
  pub duration: Option<Loose>,
  #[serde(default)]
  pub minutes: Option<u32>,
  #[serde(default)]
  pub hours: Option<f64>,
  #[serde(default, alias = "preset_id", alias = "task")]
  pub preset: Option<String>,
  #[serde(default, alias = "project_id")]
  pub project: Option<Loose>,
  #[serde(default, alias = "activity_id")]
  pub activity: Option<Loose>,
  #[serde(default, alias = "description", alias = "remarque", alias = "comment_text")]
  pub comment: Option<String>,
  #[serde(default)]
  pub source: Option<Source>,
  #[serde(default)]
  pub toggl_id: Option<i64>,
}

/// Une ou plusieurs saisies.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EntryPayload {
  One(Box<EntryInput>),
  Many(Vec<EntryInput>),
}

/// `null` explicite vs champ absent (pour effacer un horaire avec PATCH).
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
  pub from: Option<String>,
  pub to: Option<String>,
}

impl RangeQuery {
  /// Par défaut : la semaine en cours.
  fn resolve(&self) -> ApiResult<(NaiveDate, NaiveDate)> {
    let today = Local::now().date_naive();
    let from = match &self.from {
      Some(raw) => parse_date(raw)?,
      None => monday_of(today),
    };
    let to = match &self.to {
      Some(raw) => parse_date(raw)?,
      None => from + Duration::days(6),
    };
    if to < from {
      return Err(ApiError::bad_request("l'intervalle se termine avant de commencer"));
    }
    Ok((from, to))
  }
}

// ---------------------------------------------------------------------------
// Lecture
// ---------------------------------------------------------------------------

/// Tout ce dont l'interface a besoin pour afficher une période.
pub async fn state(State(state): State<AppState>, Query(range): Query<RangeQuery>) -> ApiResult<Json<Value>> {
  let (from, to) = range.resolve()?;
  let store = &state.store;
  let settings = store.settings()?;

  // Fériés de la période, pour marquer les colonnes de la semaine.
  let mut holidays = Vec::new();
  for year in from.year()..=to.year() {
    for (date, name) in crate::validate::holidays(year, &settings) {
      if date >= from && date <= to {
        holidays.push(json!({ "date": date, "name": name }));
      }
    }
  }

  Ok(Json(json!({
    "today": Local::now().date_naive(),
    "holidays": holidays,
    "range": { "from": from, "to": to },
    "settings": settings,
    "catalog": store.catalog()?,
    "entries": store.entries_between(from, to)?,
    "timer": store.timer()?,
    "months": store.months()?.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
    "auth_required": state.auth_token.is_some(),
  })))
}

pub async fn list(State(state): State<AppState>, Query(range): Query<RangeQuery>) -> ApiResult<Json<Vec<Entry>>> {
  let (from, to) = range.resolve()?;
  Ok(Json(state.store.entries_between(from, to)?))
}

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<Settings>> {
  Ok(Json(state.store.settings()?))
}

/// Fusion partielle : seuls les champs fournis sont modifiés.
pub async fn patch_settings(State(state): State<AppState>, Json(patch): Json<Value>) -> ApiResult<Json<Settings>> {
  let _guard = state.write_lock.lock().await;
  let current = serde_json::to_value(state.store.settings()?).map_err(anyhow::Error::from)?;
  let merged = merge(current, patch);
  let settings: Settings = serde_json::from_value(merged).map_err(|e| ApiError::bad_request(format!("réglages invalides : {e}")))?;
  state.store.save_settings(&settings)?;
  state.mark_self_write();
  state.store.notify(Change::scope("settings"));
  Ok(Json(settings))
}

fn merge(mut base: Value, patch: Value) -> Value {
  match (&mut base, patch) {
    (Value::Object(base_map), Value::Object(patch_map)) => {
      for (key, value) in patch_map {
        let entry = base_map.remove(&key).unwrap_or(Value::Null);
        base_map.insert(key, merge(entry, value));
      }
      base
    }
    (_, patch) => patch,
  }
}

// ---------------------------------------------------------------------------
// Écriture
// ---------------------------------------------------------------------------

pub async fn create(State(state): State<AppState>, Json(payload): Json<EntryPayload>) -> ApiResult<Json<Value>> {
  let inputs = match payload {
    EntryPayload::One(one) => vec![*one],
    EntryPayload::Many(many) => many,
  };
  if inputs.is_empty() {
    return Err(ApiError::bad_request("aucune saisie fournie"));
  }

  let _guard = state.write_lock.lock().await;
  let catalog = state.store.catalog()?;
  let mut entries = Vec::with_capacity(inputs.len());
  for (index, input) in inputs.into_iter().enumerate() {
    entries.push(build_entry(&catalog, input).map_err(|e| ApiError::bad_request(format!("saisie {} : {e}", index + 1)))?);
  }
  let created = state.store.insert_entries(entries)?;
  state.mark_self_write();
  Ok(Json(json!({ "created": created.len(), "entries": created })))
}

pub async fn update(
  State(state): State<AppState>,
  Path(id): Path<String>,
  Json(input): Json<EntryInput>,
) -> ApiResult<Json<Entry>> {
  let _guard = state.write_lock.lock().await;
  let catalog = state.store.catalog()?;
  let Some((month, mut entry)) = state.store.find_entry(&id)? else {
    return Err(ApiError::not_found(format!("saisie « {id} » introuvable")));
  };
  apply_input(&catalog, &mut entry, input).map_err(ApiError::bad_request)?;
  entry.updated_at = Entry::now();
  state.store.replace_entry(month, entry.clone())?;
  state.mark_self_write();
  Ok(Json(entry))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
  let _guard = state.write_lock.lock().await;
  match state.store.delete_entry(&id)? {
    Some(entry) => {
      state.mark_self_write();
      Ok(Json(json!({ "deleted": 1, "entry": entry })))
    }
    None => Err(ApiError::not_found(format!("saisie « {id} » introuvable"))),
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyInput {
  /// `day` (défaut) ou `week`.
  #[serde(default)]
  pub mode: Option<String>,
  /// Jour source, ou n'importe quel jour de la semaine source.
  pub source: String,
  #[serde(default)]
  pub target: Option<String>,
  /// Plusieurs cibles à la fois (« recopie lundi sur mardi, mercredi, jeudi »).
  #[serde(default)]
  pub targets: Option<Vec<String>>,
  /// Vide la cible avant de copier.
  #[serde(default)]
  pub replace: Option<bool>,
  /// En mode semaine, ne recopie pas samedi et dimanche.
  #[serde(default)]
  pub skip_weekend: Option<bool>,
  /// Remplace le commentaire de toutes les copies.
  #[serde(default)]
  pub comment: Option<String>,
}

pub async fn copy(State(state): State<AppState>, Json(input): Json<CopyInput>) -> ApiResult<Json<Value>> {
  let mode = input.mode.as_deref().unwrap_or("day").to_lowercase();
  let source = parse_date(&input.source)?;
  let replace = input.replace.unwrap_or(false);

  let mut targets = Vec::new();
  if let Some(target) = &input.target {
    targets.push(parse_date(target)?);
  }
  for target in input.targets.iter().flatten() {
    targets.push(parse_date(target)?);
  }
  if targets.is_empty() {
    return Err(ApiError::bad_request("précise `target` (ou `targets`)"));
  }

  let _guard = state.write_lock.lock().await;
  let mut created = Vec::new();

  match mode.as_str() {
    "day" => {
      let originals = state.store.entries_between(source, source)?;
      if originals.is_empty() {
        return Err(ApiError::bad_request(format!("aucune saisie le {source}")));
      }
      for target in targets {
        if replace {
          clear_range(&state, target, target, None)?;
        }
        for original in &originals {
          created.push(duplicate(original, target, input.comment.as_deref()));
        }
      }
    }
    "week" => {
      let source_monday = monday_of(source);
      let originals = state.store.entries_between(source_monday, source_monday + Duration::days(6))?;
      if originals.is_empty() {
        return Err(ApiError::bad_request(format!("aucune saisie dans la semaine du {source_monday}")));
      }
      for target in targets {
        let target_monday = monday_of(target);
        let shift = (target_monday - source_monday).num_days();
        if replace {
          clear_range(&state, target_monday, target_monday + Duration::days(6), None)?;
        }
        for original in &originals {
          let date = original.date + Duration::days(shift);
          if input.skip_weekend.unwrap_or(false) && matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            continue;
          }
          created.push(duplicate(original, date, input.comment.as_deref()));
        }
      }
    }
    other => return Err(ApiError::bad_request(format!("mode « {other} » inconnu — utilise `day` ou `week`"))),
  }

  let created = state.store.insert_entries(created)?;
  state.mark_self_write();
  Ok(Json(json!({ "created": created.len(), "entries": created })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClearInput {
  pub from: String,
  #[serde(default)]
  pub to: Option<String>,
  /// Restreint l'effacement à un projet.
  #[serde(default)]
  pub project: Option<Loose>,
  /// Compte sans rien supprimer.
  #[serde(default)]
  pub dry_run: Option<bool>,
}

pub async fn clear(State(state): State<AppState>, Json(input): Json<ClearInput>) -> ApiResult<Json<Value>> {
  let from = parse_date(&input.from)?;
  let to = match &input.to {
    Some(raw) => parse_date(raw)?,
    None => from,
  };
  if to < from {
    return Err(ApiError::bad_request("l'intervalle se termine avant de commencer"));
  }

  let project_id = match &input.project {
    Some(reference) => {
      let catalog = state.store.catalog()?;
      Some(parse::find_project(&catalog, &reference.as_text()).into_result().map_err(ApiError::bad_request)?.id.clone())
    }
    None => None,
  };

  let matching: Vec<Entry> = state
    .store
    .entries_between(from, to)?
    .into_iter()
    .filter(|e| project_id.as_ref().is_none_or(|p| &e.project_id == p))
    .collect();

  if input.dry_run.unwrap_or(false) {
    return Ok(Json(json!({ "would_delete": matching.len(), "entries": matching })));
  }

  let _guard = state.write_lock.lock().await;
  let deleted = clear_range(&state, from, to, project_id.as_deref())?;
  state.mark_self_write();
  Ok(Json(json!({ "deleted": deleted })))
}

/// Supprime les saisies d'un intervalle, mois par mois.
fn clear_range(state: &AppState, from: NaiveDate, to: NaiveDate, project_id: Option<&str>) -> ApiResult<usize> {
  let mut deleted = 0;
  for month in YearMonth::range(from, to) {
    let mut file = state.store.month(month)?;
    let before = file.entries.len();
    file.entries.retain(|e| {
      let in_range = e.date >= from && e.date <= to;
      let same_project = project_id.is_none_or(|p| e.project_id == p);
      !(in_range && same_project)
    });
    if file.entries.len() != before {
      deleted += before - file.entries.len();
      state.store.save_month(&mut file)?;
      state.store.notify(Change::month(month));
    }
  }
  Ok(deleted)
}

fn duplicate(original: &Entry, date: NaiveDate, comment: Option<&str>) -> Entry {
  let now = Entry::now();
  Entry {
    id: Entry::new_id(),
    date,
    start: original.start,
    end: original.end,
    minutes: original.minutes,
    project_id: original.project_id.clone(),
    activity_id: original.activity_id.clone(),
    comment: comment.map(str::to_string).unwrap_or_else(|| original.comment.clone()),
    source: original.source,
    toggl_id: None,
    created_at: now,
    updated_at: now,
  }
}

// ---------------------------------------------------------------------------
// Chronomètre
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerStart {
  #[serde(default, alias = "preset_id", alias = "task")]
  pub preset: Option<String>,
  #[serde(default, alias = "project_id")]
  pub project: Option<Loose>,
  #[serde(default, alias = "activity_id")]
  pub activity: Option<Loose>,
  #[serde(default, alias = "description")]
  pub comment: Option<String>,
  /// Heure de départ si le chronomètre est lancé après coup (`"08:30"`).
  #[serde(default)]
  pub started_at: Option<String>,
}

pub async fn get_timer(State(state): State<AppState>) -> ApiResult<Json<Option<RunningTimer>>> {
  Ok(Json(state.store.timer()?))
}

pub async fn start_timer(State(state): State<AppState>, Json(input): Json<TimerStart>) -> ApiResult<Json<RunningTimer>> {
  let _guard = state.write_lock.lock().await;
  if let Some(running) = state.store.timer()? {
    return Err(ApiError::conflict(format!(
      "un chronomètre tourne déjà depuis {} — arrête-le d'abord",
      running.started_at.format("%d.%m.%Y %H:%M")
    )));
  }

  let catalog = state.store.catalog()?;
  let (project_id, activity_id, preset_comment) =
    resolve_target(&catalog, input.preset.as_deref(), input.project.as_ref(), input.activity.as_ref())
      .map_err(ApiError::bad_request)?;

  let started_at = match &input.started_at {
    Some(raw) => {
      let time = opt_hhmm::parse_hhmm(raw).map_err(ApiError::bad_request)?;
      Local::now().date_naive().and_time(time)
    }
    None => Entry::now(),
  };

  let timer = RunningTimer {
    project_id,
    activity_id,
    comment: input.comment.or(preset_comment).unwrap_or_default(),
    started_at,
  };
  state.store.save_timer(Some(&timer))?;
  state.mark_self_write();
  state.store.notify(Change::scope("timer"));
  Ok(Json(timer))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerStop {
  #[serde(default, alias = "description")]
  pub comment: Option<String>,
  /// Arrondit la durée au multiple de minutes indiqué (5, 15…).
  #[serde(default)]
  pub round_to: Option<u32>,
  /// Jette la saisie au lieu de l'enregistrer.
  #[serde(default)]
  pub discard: Option<bool>,
}

pub async fn stop_timer(State(state): State<AppState>, body: Option<Json<TimerStop>>) -> ApiResult<Json<Value>> {
  let input = body.map(|Json(v)| v).unwrap_or_default();
  let _guard = state.write_lock.lock().await;
  let Some(timer) = state.store.timer()? else {
    return Err(ApiError::not_found("aucun chronomètre en cours"));
  };

  state.store.save_timer(None)?;
  state.mark_self_write();
  state.store.notify(Change::scope("timer"));

  if input.discard.unwrap_or(false) {
    return Ok(Json(json!({ "discarded": true })));
  }

  let now = Entry::now();
  let mut minutes = (now - timer.started_at).num_minutes().max(1) as u32;
  if let Some(round_to) = input.round_to.filter(|r| *r > 0) {
    minutes = ((minutes as f64 / round_to as f64).round() as u32).max(1) * round_to;
  }

  let start = timer.started_at.time();
  let entry = Entry {
    id: Entry::new_id(),
    date: timer.started_at.date(),
    start: Some(start),
    end: Some(start + Duration::minutes(minutes as i64)),
    minutes,
    project_id: timer.project_id,
    activity_id: timer.activity_id,
    comment: input.comment.unwrap_or(timer.comment),
    source: Source::Timer,
    toggl_id: None,
    created_at: now,
    updated_at: now,
  };
  let created = state.store.insert_entries(vec![entry])?;
  Ok(Json(json!({ "created": 1, "entries": created })))
}

// ---------------------------------------------------------------------------
// Construction des saisies
// ---------------------------------------------------------------------------

/// Construit une saisie complète à partir d'une description libre.
pub fn build_entry(catalog: &crate::model::Catalog, input: EntryInput) -> Result<Entry, String> {
  let date = match &input.date {
    Some(raw) => parse_date_str(raw)?,
    None => Local::now().date_naive(),
  };

  let (project_id, activity_id, preset_comment) =
    resolve_target(catalog, input.preset.as_deref(), input.project.as_ref(), input.activity.as_ref())?;

  let start = match &input.start {
    Some(Some(raw)) => Some(opt_hhmm::parse_hhmm(raw)?),
    _ => None,
  };
  let end = match &input.end {
    Some(Some(raw)) => Some(opt_hhmm::parse_hhmm(raw)?),
    _ => None,
  };

  let minutes = resolve_minutes(&input, start, end)?;
  let end = end.or_else(|| start.map(|s| s + Duration::minutes(minutes as i64)));

  let now = Entry::now();
  Ok(Entry {
    id: input.id.unwrap_or_else(Entry::new_id),
    date,
    start,
    end,
    minutes,
    project_id,
    activity_id,
    comment: input.comment.or(preset_comment).unwrap_or_default(),
    source: input.source.unwrap_or(Source::Claude),
    toggl_id: input.toggl_id,
    created_at: now,
    updated_at: now,
  })
}

/// Applique une modification partielle à une saisie existante.
fn apply_input(catalog: &crate::model::Catalog, entry: &mut Entry, mut input: EntryInput) -> Result<(), String> {
  if let Some(raw) = &input.date {
    entry.date = parse_date_str(raw)?;
  }
  if let Some(preset) = &input.preset {
    let preset = parse::find_preset(catalog, preset).into_result()?;
    entry.project_id = preset.project_id.clone();
    entry.activity_id = preset.activity_id.clone();
  }
  if let Some(project) = &input.project {
    entry.project_id = parse::find_project(catalog, &project.as_text()).into_result()?.id.clone();
  }
  if let Some(activity) = &input.activity {
    entry.activity_id = parse::find_activity(catalog, &activity.as_text()).into_result()?.id.clone();
  }
  if let Some(comment) = input.comment.take() {
    entry.comment = comment;
  }
  if let Some(start) = &input.start {
    entry.start = match start {
      Some(raw) => Some(opt_hhmm::parse_hhmm(raw)?),
      None => None,
    };
  }
  if let Some(end) = &input.end {
    entry.end = match end {
      Some(raw) => Some(opt_hhmm::parse_hhmm(raw)?),
      None => None,
    };
  }

  // La durée suit ce qui a été fourni : explicite, sinon déduite des horaires.
  if input.minutes.is_some() || input.hours.is_some() || input.duration.is_some() {
    entry.minutes = resolve_minutes(&input, entry.start, entry.end)?;
    if let Some(start) = entry.start {
      entry.end = Some(start + Duration::minutes(entry.minutes as i64));
    }
  } else if let (Some(start), Some(end)) = (entry.start, entry.end) {
    entry.minutes = span_minutes(start, end)?;
  }

  if entry.minutes == 0 {
    return Err("durée nulle".to_string());
  }
  Ok(())
}

fn resolve_minutes(input: &EntryInput, start: Option<NaiveTime>, end: Option<NaiveTime>) -> Result<u32, String> {
  if let Some(minutes) = input.minutes {
    return check_minutes(minutes);
  }
  if let Some(hours) = input.hours {
    return check_minutes((hours * 60.0).round() as u32);
  }
  if let Some(duration) = &input.duration {
    return parse::parse_minutes(&duration.as_text()).and_then(check_minutes);
  }
  match (start, end) {
    (Some(start), Some(end)) => span_minutes(start, end),
    _ => Err("durée manquante — indique `duration` (« 1h30 »), `minutes`, `hours`, ou `start` et `end`".to_string()),
  }
}

fn span_minutes(start: NaiveTime, end: NaiveTime) -> Result<u32, String> {
  let minutes = (end - start).num_minutes();
  if minutes <= 0 {
    return Err(format!("l'heure de fin ({end}) n'est pas après l'heure de début ({start})"));
  }
  check_minutes(minutes as u32)
}

fn check_minutes(minutes: u32) -> Result<u32, String> {
  if minutes == 0 {
    Err("durée nulle".to_string())
  } else if minutes > 24 * 60 {
    Err("durée supérieure à 24 h".to_string())
  } else {
    Ok(minutes)
  }
}

/// Détermine (projet, activité, commentaire par défaut) à partir d'une tâche
/// et/ou de références explicites, ces dernières l'emportant.
fn resolve_target(
  catalog: &crate::model::Catalog,
  preset: Option<&str>,
  project: Option<&Loose>,
  activity: Option<&Loose>,
) -> Result<(String, String, Option<String>), String> {
  let mut project_id = None;
  let mut activity_id = None;
  let mut comment = None;

  if let Some(preset) = preset {
    let preset = parse::find_preset(catalog, preset).into_result()?;
    project_id = Some(preset.project_id.clone());
    activity_id = Some(preset.activity_id.clone());
    // À défaut de commentaire propre à la tâche, son libellé fait un bon défaut :
    // c'est ce qui finira dans la colonne « Remarque » de SageX.
    comment = Some(preset.comment.clone().unwrap_or_else(|| preset.label.clone()));
  }
  if let Some(project) = project {
    project_id = Some(parse::find_project(catalog, &project.as_text()).into_result()?.id.clone());
  }
  if let Some(activity) = activity {
    activity_id = Some(parse::find_activity(catalog, &activity.as_text()).into_result()?.id.clone());
  }

  match (project_id, activity_id) {
    (Some(p), Some(a)) => Ok((p, a, comment)),
    (None, _) => Err("projet manquant — indique `preset` ou `project`".to_string()),
    (_, None) => Err("activité manquante — indique `preset` ou `activity`".to_string()),
  }
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

pub fn parse_date(raw: &str) -> ApiResult<NaiveDate> {
  parse_date_str(raw).map_err(ApiError::bad_request)
}

/// ISO `2026-09-16`, `16.09.2026`, `16/09/2026`, ou `today` / `yesterday` /
/// `aujourd'hui` / `hier` / `demain`.
pub fn parse_date_str(raw: &str) -> Result<NaiveDate, String> {
  let raw = raw.trim();
  let today = Local::now().date_naive();
  match raw.to_lowercase().as_str() {
    "today" | "aujourd'hui" | "aujourdhui" => return Ok(today),
    "yesterday" | "hier" => return Ok(today - Duration::days(1)),
    "tomorrow" | "demain" => return Ok(today + Duration::days(1)),
    _ => {}
  }
  for format in ["%Y-%m-%d", "%d.%m.%Y", "%d/%m/%Y", "%d-%m-%Y"] {
    if let Ok(date) = NaiveDate::parse_from_str(raw, format) {
      return Ok(date);
    }
  }
  Err(format!("date « {raw} » incomprise — format attendu AAAA-MM-JJ"))
}

pub fn monday_of(date: NaiveDate) -> NaiveDate {
  date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TogglImportInput {
  #[serde(default)]
  pub from: Option<String>,
  #[serde(default)]
  pub to: Option<String>,
  /// Raccourci pour un mois entier (`"2026-07"`).
  #[serde(default)]
  pub month: Option<String>,
  #[serde(default)]
  pub dry_run: Option<bool>,
}

/// Rapatrie une période depuis Toggl Track.
pub async fn import_toggl(State(state): State<AppState>, Json(input): Json<TogglImportInput>) -> ApiResult<Json<Value>> {
  let (from, to) = match (&input.from, &input.to, &input.month) {
    (_, _, Some(month)) => {
      let month: YearMonth = month.parse().map_err(|e: crate::model::ParseYearMonthError| ApiError::bad_request(e.to_string()))?;
      (month.first_day(), month.last_day())
    }
    (Some(from), Some(to), None) => (parse_date(from)?, parse_date(to)?),
    _ => return Err(ApiError::bad_request("précise `month`, ou `from` et `to`")),
  };

  let dry_run = input.dry_run.unwrap_or(false);
  let _guard = state.write_lock.lock().await;
  let summary = crate::toggl::run(&state.store, from, to, dry_run)
    .await
    .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
  state.mark_self_write();
  Ok(Json(json!({ "dry_run": dry_run, "from": from, "to": to, "summary": summary })))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Activity, Catalog, Preset, Project};

  fn catalog() -> Catalog {
    Catalog {
      projects: vec![Project {
        id: "spl-base".into(),
        sagex_number: 89403,
        name: "SPL Base".into(),
        color: "#000".into(),
        archived: false,
        exportable: true,
        note: None,
      }],
      activities: vec![
        Activity { id: "seance".into(), sagex_number: 4, name: "Séance de travail".into(), archived: false, note: None },
        Activity { id: "ra-d".into(), sagex_number: 29, name: "Ra&D".into(), archived: false, note: None },
      ],
      presets: vec![Preset {
        id: "constellium-daily".into(),
        label: "Daily".into(),
        project_id: "spl-base".into(),
        activity_id: "seance".into(),
        comment: Some("Daily".into()),
        color: None,
        toggl: None,
        archived: false,
      }],
    }
  }

  fn input(json: &str) -> EntryInput {
    serde_json::from_str(json).unwrap()
  }

  #[test]
  fn preset_label_is_the_default_comment() {
    // La tâche « Ra&D Subice » n'a pas de commentaire propre : son libellé sert de défaut.
    let mut catalog = catalog();
    catalog.presets[0].comment = None;
    let entry = build_entry(&catalog, input(r#"{"date":"2026-09-16","preset":"Daily","duration":"1h"}"#)).unwrap();
    assert_eq!(entry.comment, "Daily");
  }

  #[test]
  fn preset_fills_project_activity_and_comment() {
    let entry = build_entry(&catalog(), input(r#"{"date":"2026-09-16","preset":"Daily","duration":"30m"}"#)).unwrap();
    assert_eq!(entry.project_id, "spl-base");
    assert_eq!(entry.activity_id, "seance");
    assert_eq!(entry.comment, "Daily");
    assert_eq!(entry.minutes, 30);
    assert!(entry.start.is_none());
  }

  #[test]
  fn explicit_references_override_the_preset() {
    let entry = build_entry(
      &catalog(),
      input(r#"{"date":"2026-09-16","preset":"Daily","activity":29,"comment":"Biblio","hours":1.5}"#),
    )
    .unwrap();
    assert_eq!(entry.activity_id, "ra-d");
    assert_eq!(entry.comment, "Biblio");
    assert_eq!(entry.minutes, 90);
  }

  #[test]
  fn end_is_derived_from_start_and_duration() {
    let entry = build_entry(
      &catalog(),
      input(r#"{"date":"2026-09-16","project":"SPL Base","activity":"seance","start":"08:30","duration":"1h30"}"#),
    )
    .unwrap();
    assert_eq!(entry.start.unwrap().format("%H:%M").to_string(), "08:30");
    assert_eq!(entry.end.unwrap().format("%H:%M").to_string(), "10:00");
    assert_eq!(entry.minutes, 90);
  }

  #[test]
  fn duration_is_derived_from_start_and_end() {
    let entry = build_entry(
      &catalog(),
      input(r#"{"date":"2026-09-16","project":89403,"activity":4,"start":"08:00","end":"11:45"}"#),
    )
    .unwrap();
    assert_eq!(entry.minutes, 225);
  }

  #[test]
  fn missing_pieces_produce_actionable_errors() {
    let err = build_entry(&catalog(), input(r#"{"date":"2026-09-16","project":"SPL Base","activity":4}"#)).unwrap_err();
    assert!(err.contains("durée manquante"), "{err}");

    let err = build_entry(&catalog(), input(r#"{"date":"2026-09-16","duration":"1h"}"#)).unwrap_err();
    assert!(err.contains("projet manquant"), "{err}");

    let err = build_entry(&catalog(), input(r#"{"date":"2026-09-16","project":"Inconnu","duration":"1h"}"#)).unwrap_err();
    assert!(err.contains("introuvable"), "{err}");

    let err = build_entry(
      &catalog(),
      input(r#"{"date":"2026-09-16","preset":"Daily","start":"10:00","end":"09:00"}"#),
    )
    .unwrap_err();
    assert!(err.contains("pas après"), "{err}");
  }

  #[test]
  fn patch_clears_times_with_an_explicit_null() {
    let mut entry = build_entry(
      &catalog(),
      input(r#"{"date":"2026-09-16","preset":"Daily","start":"08:00","duration":"1h"}"#),
    )
    .unwrap();
    apply_input(&catalog(), &mut entry, input(r#"{"start":null,"end":null}"#)).unwrap();
    assert!(entry.start.is_none() && entry.end.is_none());
    assert_eq!(entry.minutes, 60, "la durée survit à la suppression des horaires");

    apply_input(&catalog(), &mut entry, input(r#"{"duration":"2h"}"#)).unwrap();
    assert_eq!(entry.minutes, 120);
  }

  #[test]
  fn dates_accept_several_spellings() {
    assert_eq!(parse_date_str("2026-09-16").unwrap(), NaiveDate::from_ymd_opt(2026, 9, 16).unwrap());
    assert_eq!(parse_date_str("16.09.2026").unwrap(), NaiveDate::from_ymd_opt(2026, 9, 16).unwrap());
    assert_eq!(parse_date_str("hier").unwrap(), Local::now().date_naive() - Duration::days(1));
    assert!(parse_date_str("le 16").is_err());
  }

  #[test]
  fn monday_is_the_first_day_of_the_week() {
    let sunday = NaiveDate::from_ymd_opt(2026, 9, 20).unwrap();
    assert_eq!(monday_of(sunday), NaiveDate::from_ymd_opt(2026, 9, 14).unwrap());
    let monday = NaiveDate::from_ymd_opt(2026, 9, 14).unwrap();
    assert_eq!(monday_of(monday), monday);
  }
}
