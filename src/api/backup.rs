//! Backup and restore of a whole base, as a zip archive.

use crate::api::{ApiError, ApiResult};
use crate::app::Space;
use crate::store::Change;
use axum::body::Bytes;
use axum::extract::Query;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Local;
use serde::Deserialize;
use serde_json::json;
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;

/// The files of a base, as paths relative to its folder.
fn database_files(space: &Space) -> ApiResult<Vec<(String, std::path::PathBuf)>> {
  let mut files = vec![
    ("catalog.json".to_string(), space.store.catalog_path()),
    ("settings.json".to_string(), space.store.settings_path()),
  ];
  for month in space.store.months()? {
    files.push((format!("entries/{month}.json"), space.store.month_path(month)));
  }
  let timer = space.store.timer_path();
  if timer.exists() {
    files.push(("timer.json".to_string(), timer));
  }
  Ok(files.into_iter().filter(|(_, path)| path.exists()).collect())
}

fn build_archive(space: &Space) -> ApiResult<Vec<u8>> {
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

  for (name, path) in database_files(space)? {
    let content = std::fs::read(&path).map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    writer.start_file(name, options).map_err(|e| anyhow::anyhow!("archive: {e}"))?;
    writer.write_all(&content).map_err(|e| anyhow::anyhow!("archive: {e}"))?;
  }

  let cursor = writer.finish().map_err(|e| anyhow::anyhow!("archive: {e}"))?;
  Ok(cursor.into_inner())
}

/// Downloads the whole base and notes the date of the backup.
pub async fn download(space: Space) -> ApiResult<Response> {
  let bytes = build_archive(&space)?;

  {
    let _guard = space.write_lock.lock().await;
    let mut settings = space.store.settings()?;
    settings.last_backup = Some(Local::now().date_naive());
    space.store.save_settings(&settings)?;
    space.mark_self_write();
    space.store.notify(Change::scope("settings"));
  }

  let name = format!("sagex-hours-{}.zip", Local::now().format("%Y-%m-%d"));
  Ok((
    [
      (header::CONTENT_TYPE, "application/zip".to_string()),
      (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
    ],
    bytes,
  )
    .into_response())
}

#[derive(Debug, Deserialize)]
pub struct RestoreQuery {
  /// Describes what the archive holds without writing anything.
  #[serde(default)]
  pub dry_run: Option<bool>,
}

/// Restores an archive made by `GET /api/backup`.
///
/// The current base is first copied into `backups/`: a restore cannot lose the
/// state in place.
pub async fn restore(
  space: Space,
  Query(query): Query<RestoreQuery>,
  body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
  if body.is_empty() {
    return Err(ApiError::bad_request(tr!("empty archive — send the zip as the request body")));
  }

  let mut archive =
    zip::ZipArchive::new(Cursor::new(body.clone())).map_err(|e| ApiError::bad_request(tr!("unreadable archive: {error}", error = e)))?;

  let mut contents: Vec<(String, Vec<u8>)> = Vec::new();
  for index in 0..archive.len() {
    let mut file = archive.by_index(index).map_err(|e| ApiError::bad_request(tr!("unreadable archive: {error}", error = e)))?;
    let name = file.name().to_string();
    // Only the expected files, and never a path that climbs out.
    let accepted = name == "catalog.json" || name == "settings.json" || name == "timer.json" || name.starts_with("entries/");
    if !accepted || name.contains("..") || name.starts_with('/') {
      continue;
    }
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| anyhow::anyhow!("reading {name}: {e}"))?;
    // Invalid JSON is refused before anything is touched.
    serde_json::from_slice::<serde_json::Value>(&buffer)
      .map_err(|e| ApiError::bad_request(tr!("{name} is not valid JSON: {error}", name = name, error = e)))?;
    contents.push((name, buffer));
  }

  if contents.is_empty() {
    return Err(ApiError::bad_request(tr!("the archive holds no file of the base")));
  }

  let months: Vec<&String> = contents.iter().map(|(name, _)| name).filter(|n| n.starts_with("entries/")).collect();
  if query.dry_run.unwrap_or(false) {
    return Ok(Json(json!({
      "dry_run": true,
      "files": contents.iter().map(|(name, _)| name).collect::<Vec<_>>(),
      "months": months.len(),
    })));
  }

  let _guard = space.write_lock.lock().await;

  // A safety net before overwriting.
  let safety = build_archive(&space)?;
  let safety_path = space.store.backups_dir().join(format!("before-restore-{}.zip", Local::now().format("%Y-%m-%d-%H%M%S")));
  std::fs::create_dir_all(space.store.backups_dir()).map_err(|e| anyhow::anyhow!("creating the backups folder: {e}"))?;
  std::fs::write(&safety_path, safety).map_err(|e| anyhow::anyhow!("writing {}: {e}", safety_path.display()))?;

  // Months missing from the archive disappear: a restore is a replacement.
  for month in space.store.months()? {
    let path = space.store.month_path(month);
    if path.exists() {
      std::fs::remove_file(&path).map_err(|e| anyhow::anyhow!("deleting {}: {e}", path.display()))?;
    }
  }

  let root = space.store.root().to_path_buf();
  for (name, content) in &contents {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| anyhow::anyhow!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, content).map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
  }

  space.mark_self_write();
  space.store.notify(Change::scope("all"));

  Ok(Json(json!({
    "restored": contents.len(),
    "months": months.len(),
    "safety_backup": safety_path.file_name().map(|n| n.to_string_lossy().to_string()),
  })))
}
