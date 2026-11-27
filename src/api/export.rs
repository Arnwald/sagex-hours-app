//! Monthly report and download of the SageX import file.

use crate::api::{ApiError, ApiResult};
use crate::app::Space;
use crate::export;
use crate::model::YearMonth;
use crate::settings::{Aggregate, CommentStyle, Settings};
use crate::validate::{self, Report};
use axum::extract::{Path, Query};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
  /// `month`, `day` or `none`; by default, the value of the settings.
  #[serde(default)]
  pub aggregate: Option<String>,
  #[serde(default)]
  pub comment_style: Option<String>,
  /// Downloads in spite of the errors reported.
  #[serde(default)]
  pub force: Option<bool>,
}

impl ExportQuery {
  /// Applies one-off options over the saved settings.
  fn apply(&self, mut settings: Settings) -> ApiResult<Settings> {
    if let Some(raw) = &self.aggregate {
      settings.export.aggregate = match raw.to_lowercase().as_str() {
        "month" | "mois" => Aggregate::Month,
        "day" | "jour" => Aggregate::Day,
        "none" | "aucune" => Aggregate::None,
        other => return Err(ApiError::bad_request(tr!("unknown aggregation “{value}”", value = other))),
      };
    }
    if let Some(raw) = &self.comment_style {
      settings.export.comment_style = match raw.to_lowercase().as_str() {
        "detailed" | "detaille" => CommentStyle::Detailed,
        "compact" => CommentStyle::Compact,
        "auto" => CommentStyle::Auto,
        other => return Err(ApiError::bad_request(tr!("unknown comment style “{value}”", value = other))),
      };
    }
    Ok(settings)
  }
}

fn month_of(raw: &str) -> ApiResult<YearMonth> {
  raw.trim_end_matches(".xlsx").parse::<YearMonth>().map_err(|e| ApiError::bad_request(e.to_string()))
}

/// Totals, checks and a preview of the rows that will go to SageX.
pub async fn month_report(
  space: Space,
  Path(raw_month): Path<String>,
  Query(query): Query<ExportQuery>,
) -> ApiResult<Json<serde_json::Value>> {
  let month = month_of(&raw_month)?;
  let settings = query.apply(space.store.settings()?)?;
  let catalog = space.store.catalog()?;
  let entries = space.store.month(month)?.entries;

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

/// The `.xlsx` file ready for *Heures → Transfert des heures* in SageX.
pub async fn download(
  space: Space,
  Path(raw_month): Path<String>,
  Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
  let month = month_of(&raw_month)?;
  let settings = query.apply(space.store.settings()?)?;
  let catalog = space.store.catalog()?;
  let entries = space.store.month(month)?.entries;

  let report = validate::report(month, &entries, &catalog, &settings);
  if report.has_errors() && !query.force.unwrap_or(false) {
    let blocking: Vec<_> = report.issues.iter().filter(|i| i.severity == validate::Severity::Error).collect();
    return Err(ApiError {
      status: StatusCode::CONFLICT,
      message: tr!(
        "{count} error(s) block the export: {errors} — fix them, or retry with ?force=true",
        count = blocking.len(),
        errors = blocking.iter().map(|i| i.message.as_str()).collect::<Vec<_>>().join(" ; ")
      ),
    });
  }

  let built = export::build(&entries, &catalog, &settings, Some(month));
  if built.rows.is_empty() {
    return Err(ApiError::bad_request(tr!("no line to export for {month}", month = month)));
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
