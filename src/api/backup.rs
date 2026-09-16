//! Sauvegarde et restauration de la base complète, sous forme d'une archive zip.

use crate::api::{ApiError, ApiResult};
use crate::app::AppState;
use crate::store::Change;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Local;
use serde::Deserialize;
use serde_json::json;
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;

/// Fichiers de la base, chemins relatifs à la racine du dossier de données.
fn database_files(state: &AppState) -> ApiResult<Vec<(String, std::path::PathBuf)>> {
  let root = state.store.root();
  let mut files = vec![
    ("catalog.json".to_string(), state.store.catalog_path()),
    ("settings.json".to_string(), state.store.settings_path()),
  ];
  for month in state.store.months()? {
    files.push((format!("entries/{month}.json"), state.store.month_path(month)));
  }
  let timer = state.store.timer_path();
  if timer.exists() {
    files.push(("timer.json".to_string(), timer));
  }
  let _ = root;
  Ok(files.into_iter().filter(|(_, path)| path.exists()).collect())
}

fn build_archive(state: &AppState) -> ApiResult<Vec<u8>> {
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

  for (name, path) in database_files(state)? {
    let content = std::fs::read(&path).map_err(|e| anyhow::anyhow!("lecture de {} : {e}", path.display()))?;
    writer.start_file(name, options).map_err(|e| anyhow::anyhow!("archive : {e}"))?;
    writer.write_all(&content).map_err(|e| anyhow::anyhow!("archive : {e}"))?;
  }

  let cursor = writer.finish().map_err(|e| anyhow::anyhow!("archive : {e}"))?;
  Ok(cursor.into_inner())
}

/// Télécharge la base complète et note la date de sauvegarde.
pub async fn download(State(state): State<AppState>) -> ApiResult<Response> {
  let bytes = build_archive(&state)?;

  {
    let _guard = state.write_lock.lock().await;
    let mut settings = state.store.settings()?;
    settings.last_backup = Some(Local::now().date_naive());
    state.store.save_settings(&settings)?;
    state.mark_self_write();
    state.store.notify(Change::scope("settings"));
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
  /// Décrit ce que contient l'archive sans rien écrire.
  #[serde(default)]
  pub dry_run: Option<bool>,
}

/// Restaure une archive produite par `GET /api/backup`.
///
/// La base actuelle est d'abord copiée dans `backups/` : une restauration ne
/// peut pas faire perdre l'état en place.
pub async fn restore(
  State(state): State<AppState>,
  Query(query): Query<RestoreQuery>,
  body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
  if body.is_empty() {
    return Err(ApiError::bad_request("archive vide — envoie le zip dans le corps de la requête"));
  }

  let mut archive =
    zip::ZipArchive::new(Cursor::new(body.clone())).map_err(|e| ApiError::bad_request(format!("archive illisible : {e}")))?;

  let mut contents: Vec<(String, Vec<u8>)> = Vec::new();
  for index in 0..archive.len() {
    let mut file = archive.by_index(index).map_err(|e| ApiError::bad_request(format!("archive illisible : {e}")))?;
    let name = file.name().to_string();
    // On n'accepte que les fichiers attendus, jamais un chemin qui remonte.
    let accepted = name == "catalog.json" || name == "settings.json" || name == "timer.json" || name.starts_with("entries/");
    if !accepted || name.contains("..") || name.starts_with('/') {
      continue;
    }
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| anyhow::anyhow!("lecture de {name} : {e}"))?;
    // Un JSON invalide est refusé avant d'avoir touché quoi que ce soit.
    serde_json::from_slice::<serde_json::Value>(&buffer)
      .map_err(|e| ApiError::bad_request(format!("{name} n'est pas un JSON valide : {e}")))?;
    contents.push((name, buffer));
  }

  if contents.is_empty() {
    return Err(ApiError::bad_request("l'archive ne contient aucun fichier de la base"));
  }

  let months: Vec<&String> = contents.iter().map(|(name, _)| name).filter(|n| n.starts_with("entries/")).collect();
  if query.dry_run.unwrap_or(false) {
    return Ok(Json(json!({
      "dry_run": true,
      "files": contents.iter().map(|(name, _)| name).collect::<Vec<_>>(),
      "months": months.len(),
    })));
  }

  let _guard = state.write_lock.lock().await;

  // Filet de sécurité avant écrasement.
  let safety = build_archive(&state)?;
  let safety_path = state.store.backups_dir().join(format!("avant-restauration-{}.zip", Local::now().format("%Y-%m-%d-%H%M%S")));
  std::fs::create_dir_all(state.store.backups_dir()).map_err(|e| anyhow::anyhow!("création du dossier de sauvegardes : {e}"))?;
  std::fs::write(&safety_path, safety).map_err(|e| anyhow::anyhow!("écriture de {} : {e}", safety_path.display()))?;

  // Les mois absents de l'archive disparaissent : la restauration est un remplacement.
  for month in state.store.months()? {
    let path = state.store.month_path(month);
    if path.exists() {
      std::fs::remove_file(&path).map_err(|e| anyhow::anyhow!("suppression de {} : {e}", path.display()))?;
    }
  }

  let root = state.store.root().to_path_buf();
  for (name, content) in &contents {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| anyhow::anyhow!("création de {} : {e}", parent.display()))?;
    }
    std::fs::write(&path, content).map_err(|e| anyhow::anyhow!("écriture de {} : {e}", path.display()))?;
  }

  state.mark_self_write();
  state.store.notify(Change::scope("all"));

  Ok(Json(json!({
    "restored": contents.len(),
    "months": months.len(),
    "safety_backup": safety_path.file_name().map(|n| n.to_string_lossy().to_string()),
  })))
}
