//! Entries: reading, creating (one or many), editing, copying days or weeks,
//! and the timer.

use crate::api::{ApiError, ApiResult};
use crate::app::{Caller, Space};
use crate::model::{opt_hhmm, Entry, RunningTimer, Source, YearMonth};
use crate::parse::{self, Loose};
use crate::settings::Settings;
use crate::store::Change;
use axum::extract::{Path, Query};
use axum::Json;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Weekday};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

/// An entry as one would write it by hand.
///
/// References accept the identifier, the SageX number or the name; the duration
/// accepts `1h30`, `90`, `1.5h`… Aliases cover the natural spellings
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

/// One entry or several.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EntryPayload {
  One(Box<EntryInput>),
  Many(Vec<EntryInput>),
}

/// An explicit `null` vs a missing field (to clear a time with PATCH).
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
  /// By default, the current week.
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
      return Err(ApiError::bad_request(tr!("the range ends before it starts")));
    }
    Ok((from, to))
  }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Everything the interface needs to show a period.
pub async fn state(caller: Caller, Query(range): Query<RangeQuery>) -> ApiResult<Json<Value>> {
  let (from, to) = range.resolve()?;
  let store = &caller.space.store;
  let settings = store.settings()?;

  // The period's holidays, to mark the week's columns.
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
    "me": caller.info(),
  })))
}

pub async fn list(space: Space, Query(range): Query<RangeQuery>) -> ApiResult<Json<Vec<Entry>>> {
  let (from, to) = range.resolve()?;
  Ok(Json(space.store.entries_between(from, to)?))
}

pub async fn get_settings(space: Space) -> ApiResult<Json<Settings>> {
  Ok(Json(space.store.settings()?))
}

/// Partial merge: only the fields given change.
pub async fn patch_settings(space: Space, Json(patch): Json<Value>) -> ApiResult<Json<Settings>> {
  let _guard = space.write_lock.lock().await;
  let current = serde_json::to_value(space.store.settings()?).map_err(anyhow::Error::from)?;
  let merged = merge(current, patch);
  let settings: Settings =
    serde_json::from_value(merged).map_err(|e| ApiError::bad_request(tr!("invalid settings: {error}", error = e)))?;
  space.store.save_settings(&settings)?;
  space.mark_self_write();
  space.store.notify(Change::scope("settings"));
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
// Writing
// ---------------------------------------------------------------------------

pub async fn create(space: Space, Json(payload): Json<EntryPayload>) -> ApiResult<Json<Value>> {
  let inputs = match payload {
    EntryPayload::One(one) => vec![*one],
    EntryPayload::Many(many) => many,
  };
  if inputs.is_empty() {
    return Err(ApiError::bad_request(tr!("no entry given")));
  }

  let _guard = space.write_lock.lock().await;
  let catalog = space.store.catalog()?;
  let mut entries = Vec::with_capacity(inputs.len());
  for (index, input) in inputs.into_iter().enumerate() {
    entries.push(build_entry(&catalog, input).map_err(|e| ApiError::bad_request(tr!("entry {n}: {error}", n = index + 1, error = e)))?);
  }
  let created = space.store.insert_entries(entries)?;
  space.mark_self_write();
  Ok(Json(json!({ "created": created.len(), "entries": created })))
}

pub async fn update(space: Space, Path(id): Path<String>, Json(input): Json<EntryInput>) -> ApiResult<Json<Entry>> {
  let _guard = space.write_lock.lock().await;
  let catalog = space.store.catalog()?;
  let Some((month, mut entry)) = space.store.find_entry(&id)? else {
    return Err(ApiError::not_found(tr!("entry “{id}” not found", id = id)));
  };
  apply_input(&catalog, &mut entry, input).map_err(ApiError::bad_request)?;
  entry.updated_at = Entry::now();
  space.store.replace_entry(month, entry.clone())?;
  space.mark_self_write();
  Ok(Json(entry))
}

pub async fn remove(space: Space, Path(id): Path<String>) -> ApiResult<Json<Value>> {
  let _guard = space.write_lock.lock().await;
  match space.store.delete_entry(&id)? {
    Some(entry) => {
      space.mark_self_write();
      Ok(Json(json!({ "deleted": 1, "entry": entry })))
    }
    None => Err(ApiError::not_found(tr!("entry “{id}” not found", id = id))),
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyInput {
  /// `day` (default) or `week`.
  #[serde(default)]
  pub mode: Option<String>,
  /// The source day, or any day of the source week.
  pub source: String,
  #[serde(default)]
  pub target: Option<String>,
  /// Several targets at once ("copy Monday to Tuesday, Wednesday, Thursday").
  #[serde(default)]
  pub targets: Option<Vec<String>>,
  /// Empties the target before copying.
  #[serde(default)]
  pub replace: Option<bool>,
  /// In week mode, leaves Saturday and Sunday out.
  #[serde(default)]
  pub skip_weekend: Option<bool>,
  /// Replaces the comment of every copy.
  #[serde(default)]
  pub comment: Option<String>,
}

pub async fn copy(space: Space, Json(input): Json<CopyInput>) -> ApiResult<Json<Value>> {
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
    return Err(ApiError::bad_request(tr!("give `target` (or `targets`)")));
  }

  let _guard = space.write_lock.lock().await;
  let mut created = Vec::new();

  match mode.as_str() {
    "day" => {
      let originals = space.store.entries_between(source, source)?;
      if originals.is_empty() {
        return Err(ApiError::bad_request(tr!("no entry on {date}", date = source)));
      }
      for target in targets {
        if replace {
          clear_range(&space, target, target, None)?;
        }
        for original in &originals {
          created.push(duplicate(original, target, input.comment.as_deref()));
        }
      }
    }
    "week" => {
      let source_monday = monday_of(source);
      let originals = space.store.entries_between(source_monday, source_monday + Duration::days(6))?;
      if originals.is_empty() {
        return Err(ApiError::bad_request(tr!("no entry in the week of {date}", date = source_monday)));
      }
      for target in targets {
        let target_monday = monday_of(target);
        let shift = (target_monday - source_monday).num_days();
        if replace {
          clear_range(&space, target_monday, target_monday + Duration::days(6), None)?;
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
    other => return Err(ApiError::bad_request(tr!("unknown mode “{mode}” — use `day` or `week`", mode = other))),
  }

  let created = space.store.insert_entries(created)?;
  space.mark_self_write();
  Ok(Json(json!({ "created": created.len(), "entries": created })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClearInput {
  pub from: String,
  #[serde(default)]
  pub to: Option<String>,
  /// Limits the deletion to one project.
  #[serde(default)]
  pub project: Option<Loose>,
  /// Counts without deleting anything.
  #[serde(default)]
  pub dry_run: Option<bool>,
}

pub async fn clear(space: Space, Json(input): Json<ClearInput>) -> ApiResult<Json<Value>> {
  let from = parse_date(&input.from)?;
  let to = match &input.to {
    Some(raw) => parse_date(raw)?,
    None => from,
  };
  if to < from {
    return Err(ApiError::bad_request(tr!("the range ends before it starts")));
  }

  let project_id = match &input.project {
    Some(reference) => {
      let catalog = space.store.catalog()?;
      Some(parse::find_project(&catalog, &reference.as_text()).into_result().map_err(ApiError::bad_request)?.id.clone())
    }
    None => None,
  };

  let matching: Vec<Entry> = space
    .store
    .entries_between(from, to)?
    .into_iter()
    .filter(|e| project_id.as_ref().is_none_or(|p| &e.project_id == p))
    .collect();

  if input.dry_run.unwrap_or(false) {
    return Ok(Json(json!({ "would_delete": matching.len(), "entries": matching })));
  }

  let _guard = space.write_lock.lock().await;
  let deleted = clear_range(&space, from, to, project_id.as_deref())?;
  space.mark_self_write();
  Ok(Json(json!({ "deleted": deleted })))
}

/// Deletes the entries of a range, month by month.
fn clear_range(space: &Space, from: NaiveDate, to: NaiveDate, project_id: Option<&str>) -> ApiResult<usize> {
  let mut deleted = 0;
  for month in YearMonth::range(from, to) {
    let mut file = space.store.month(month)?;
    let before = file.entries.len();
    file.entries.retain(|e| {
      let in_range = e.date >= from && e.date <= to;
      let same_project = project_id.is_none_or(|p| e.project_id == p);
      !(in_range && same_project)
    });
    if file.entries.len() != before {
      deleted += before - file.entries.len();
      space.store.save_month(&mut file)?;
      space.store.notify(Change::month(month));
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
// Timer
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
  /// Start time when the timer is started after the fact (`"08:30"`).
  #[serde(default)]
  pub started_at: Option<String>,
}

pub async fn get_timer(space: Space) -> ApiResult<Json<Option<RunningTimer>>> {
  Ok(Json(space.store.timer()?))
}

pub async fn start_timer(space: Space, Json(input): Json<TimerStart>) -> ApiResult<Json<RunningTimer>> {
  let _guard = space.write_lock.lock().await;
  if let Some(running) = space.store.timer()? {
    return Err(ApiError::conflict(tr!(
      "a timer has been running since {since} — stop it first",
      since = running.started_at.format("%d.%m.%Y %H:%M")
    )));
  }

  let catalog = space.store.catalog()?;
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
  space.store.save_timer(Some(&timer))?;
  space.mark_self_write();
  space.store.notify(Change::scope("timer"));
  Ok(Json(timer))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerStop {
  #[serde(default, alias = "description")]
  pub comment: Option<String>,
  /// Rounds the duration to the given number of minutes (5, 15…).
  #[serde(default)]
  pub round_to: Option<u32>,
  /// Throws the entry away instead of saving it.
  #[serde(default)]
  pub discard: Option<bool>,
}

pub async fn stop_timer(space: Space, body: Option<Json<TimerStop>>) -> ApiResult<Json<Value>> {
  let input = body.map(|Json(v)| v).unwrap_or_default();
  let _guard = space.write_lock.lock().await;
  let Some(timer) = space.store.timer()? else {
    return Err(ApiError::not_found(tr!("no timer running")));
  };

  space.store.save_timer(None)?;
  space.mark_self_write();
  space.store.notify(Change::scope("timer"));

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
  let created = space.store.insert_entries(vec![entry])?;
  Ok(Json(json!({ "created": 1, "entries": created })))
}

// ---------------------------------------------------------------------------
// Building entries
// ---------------------------------------------------------------------------

/// Builds a complete entry from a free description.
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

/// Applies a partial change to an existing entry.
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

  // The duration follows what was given: explicit, else derived from the times.
  if input.minutes.is_some() || input.hours.is_some() || input.duration.is_some() {
    entry.minutes = resolve_minutes(&input, entry.start, entry.end)?;
    if let Some(start) = entry.start {
      entry.end = Some(start + Duration::minutes(entry.minutes as i64));
    }
  } else if let (Some(start), Some(end)) = (entry.start, entry.end) {
    entry.minutes = span_minutes(start, end)?;
  }

  if entry.minutes == 0 {
    return Err(tr!("zero duration"));
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
    _ => Err(tr!("missing duration — give `duration` (“1h30”), `minutes`, `hours`, or `start` and `end`")),
  }
}

fn span_minutes(start: NaiveTime, end: NaiveTime) -> Result<u32, String> {
  let minutes = (end - start).num_minutes();
  if minutes <= 0 {
    return Err(tr!(
      "the end time ({end}) is not after the start time ({start})",
      end = end.format("%H:%M"),
      start = start.format("%H:%M")
    ));
  }
  check_minutes(minutes as u32)
}

fn check_minutes(minutes: u32) -> Result<u32, String> {
  if minutes == 0 {
    Err(tr!("zero duration"))
  } else if minutes > 24 * 60 {
    Err(tr!("duration over 24 h"))
  } else {
    Ok(minutes)
  }
}

/// Works out (project, activity, default comment) from a task and/or explicit
/// references, the latter winning.
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
    // Without a comment of its own, the task's label makes a good default:
    // it is what ends up in SageX's « Remarque » column.
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
    (None, _) => Err(tr!("missing project — give `preset` or `project`")),
    (_, None) => Err(tr!("missing activity — give `preset` or `activity`")),
  }
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

pub fn parse_date(raw: &str) -> ApiResult<NaiveDate> {
  parse_date_str(raw).map_err(ApiError::bad_request)
}

/// ISO `2026-09-16`, `16.09.2026`, `16/09/2026`, or `today` / `yesterday` /
/// `tomorrow`, in English or French.
pub fn parse_date_str(raw: &str) -> Result<NaiveDate, String> {
  let raw = raw.trim();
  let today = Local::now().date_naive();
  match raw.to_lowercase().as_str() {
    "today" | "aujourd'hui" | "aujourd’hui" | "aujourdhui" => return Ok(today),
    "yesterday" | "hier" => return Ok(today - Duration::days(1)),
    "tomorrow" | "demain" => return Ok(today + Duration::days(1)),
    _ => {}
  }
  for format in ["%Y-%m-%d", "%d.%m.%Y", "%d/%m/%Y", "%d-%m-%Y"] {
    if let Ok(date) = NaiveDate::parse_from_str(raw, format) {
      return Ok(date);
    }
  }
  Err(tr!("date “{raw}” not understood — expected format YYYY-MM-DD", raw = raw))
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
  /// Shortcut for a whole month (`"2026-07"`).
  #[serde(default)]
  pub month: Option<String>,
  #[serde(default)]
  pub dry_run: Option<bool>,
}

/// Brings a period back from Toggl Track.
pub async fn import_toggl(space: Space, Json(input): Json<TogglImportInput>) -> ApiResult<Json<Value>> {
  let (from, to) = match (&input.from, &input.to, &input.month) {
    (_, _, Some(month)) => {
      let month: YearMonth = month.parse().map_err(|e: crate::model::ParseYearMonthError| ApiError::bad_request(e.to_string()))?;
      (month.first_day(), month.last_day())
    }
    (Some(from), Some(to), None) => (parse_date(from)?, parse_date(to)?),
    _ => return Err(ApiError::bad_request(tr!("give `month`, or `from` and `to`"))),
  };

  let dry_run = input.dry_run.unwrap_or(false);
  let _guard = space.write_lock.lock().await;
  let summary = crate::toggl::run(&space.store, from, to, dry_run)
    .await
    .map_err(|e| ApiError::bad_request(tr!("Toggl import: {error}", error = format!("{e:#}"))))?;
  space.mark_self_write();
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
    // A task without a comment of its own: its label is the default.
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
    assert!(err.contains("missing duration"), "{err}");

    let err = build_entry(&catalog(), input(r#"{"date":"2026-09-16","duration":"1h"}"#)).unwrap_err();
    assert!(err.contains("missing project"), "{err}");

    let err = build_entry(&catalog(), input(r#"{"date":"2026-09-16","project":"Unknown","duration":"1h"}"#)).unwrap_err();
    assert!(err.contains("not found"), "{err}");

    let err = build_entry(
      &catalog(),
      input(r#"{"date":"2026-09-16","preset":"Daily","start":"10:00","end":"09:00"}"#),
    )
    .unwrap_err();
    assert!(err.contains("not after"), "{err}");
  }

  #[tokio::test]
  async fn errors_follow_the_requests_language() {
    let err = crate::i18n::scope(crate::i18n::Lang::Fr, async {
      build_entry(&catalog(), input(r#"{"date":"2026-09-16","project":"SPL Base","activity":4}"#)).unwrap_err()
    })
    .await;
    assert!(err.starts_with("durée manquante"), "{err}");
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
    assert_eq!(entry.minutes, 60, "the duration survives clearing the times");

    apply_input(&catalog(), &mut entry, input(r#"{"duration":"2h"}"#)).unwrap();
    assert_eq!(entry.minutes, 120);
  }

  #[test]
  fn dates_accept_several_spellings() {
    assert_eq!(parse_date_str("2026-09-16").unwrap(), NaiveDate::from_ymd_opt(2026, 9, 16).unwrap());
    assert_eq!(parse_date_str("16.09.2026").unwrap(), NaiveDate::from_ymd_opt(2026, 9, 16).unwrap());
    assert_eq!(parse_date_str("hier").unwrap(), Local::now().date_naive() - Duration::days(1));
    assert_eq!(parse_date_str("Yesterday").unwrap(), Local::now().date_naive() - Duration::days(1));
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
