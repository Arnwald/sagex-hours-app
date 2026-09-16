//! Rapport mensuel et téléchargement du fichier d'import SageX.

use crate::api::{ApiError, ApiResult};
use crate::app::AppState;
use crate::export;
use crate::model::YearMonth;
use crate::settings::{Aggregate, CommentStyle, Settings};
use crate::validate::{self, Report};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
  /// `month`, `day` ou `none` ; par défaut, la valeur des réglages.
  #[serde(default)]
  pub aggregate: Option<String>,
  #[serde(default)]
  pub comment_style: Option<String>,
  /// Télécharge malgré les erreurs signalées.
  #[serde(default)]
  pub force: Option<bool>,
}

impl ExportQuery {
  /// Applique les options ponctuelles par-dessus les réglages enregistrés.
  fn apply(&self, mut settings: Settings) -> ApiResult<Settings> {
    if let Some(raw) = &self.aggregate {
      settings.export.aggregate = match raw.to_lowercase().as_str() {
        "month" | "mois" => Aggregate::Month,
        "day" | "jour" => Aggregate::Day,
        "none" | "aucune" => Aggregate::None,
        other => return Err(ApiError::bad_request(format!("agrégation « {other} » inconnue"))),
      };
    }
    if let Some(raw) = &self.comment_style {
      settings.export.comment_style = match raw.to_lowercase().as_str() {
        "detailed" | "detaille" => CommentStyle::Detailed,
        "compact" => CommentStyle::Compact,
        "auto" => CommentStyle::Auto,
        other => return Err(ApiError::bad_request(format!("style de commentaire « {other} » inconnu"))),
      };
    }
    Ok(settings)
  }
}

fn month_of(raw: &str) -> ApiResult<YearMonth> {
  raw.trim_end_matches(".xlsx").parse::<YearMonth>().map_err(|e| ApiError::bad_request(e.to_string()))
}

/// Totaux, contrôles et aperçu des lignes qui partiront dans SageX.
pub async fn month_report(
  State(state): State<AppState>,
  Path(raw_month): Path<String>,
  Query(query): Query<ExportQuery>,
) -> ApiResult<Json<serde_json::Value>> {
  let month = month_of(&raw_month)?;
  let settings = query.apply(state.store.settings()?)?;
  let catalog = state.store.catalog()?;
  let entries = state.store.month(month)?.entries;

  let report: Report = validate::report(month, &entries, &catalog, &settings);
  let preview = export::build(&entries, &catalog, &settings, Some(month));

  Ok(Json(json!({
    "report": report,
    "rows": preview.rows,
    "skipped": preview.skipped,
    "aggregate": settings.export.aggregate,
    "comment_style": settings.export.comment_style,
  })))
}

/// Fichier `.xlsx` prêt pour *Heures → Transfert des heures* dans SageX.
pub async fn download(
  State(state): State<AppState>,
  Path(raw_month): Path<String>,
  Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
  let month = month_of(&raw_month)?;
  let settings = query.apply(state.store.settings()?)?;
  let catalog = state.store.catalog()?;
  let entries = state.store.month(month)?.entries;

  let report = validate::report(month, &entries, &catalog, &settings);
  if report.has_errors() && !query.force.unwrap_or(false) {
    let blocking: Vec<_> = report.issues.iter().filter(|i| i.severity == validate::Severity::Error).collect();
    return Err(ApiError {
      status: StatusCode::CONFLICT,
      message: format!(
        "{} erreur(s) bloquent l'export : {} — corrige-les, ou relance avec ?force=true",
        blocking.len(),
        blocking.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join(" ; ")
      ),
    });
  }

  let built = export::build(&entries, &catalog, &settings, Some(month));
  if built.rows.is_empty() {
    return Err(ApiError::bad_request(format!("aucune ligne à exporter pour {month}")));
  }
  let bytes = export::to_xlsx(&built.rows, &settings)?;

  let name = settings
    .export
    .file_name
    .replace("{from}", &month.first_day().to_string())
    .replace("{to}", &month.last_day().to_string())
    .replace("{month}", &month.to_string());

  Ok((
    [
      (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
      (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}.xlsx\"")),
    ],
    bytes,
  )
    .into_response())
}
