//! The Claude Desktop extension (`.mcpb`), made for the person who downloads
//! it: a generic relay that Claude Desktop runs with its own Node.js, plus the
//! instance's address and the person's key. Installing it is a double-click,
//! with nothing else to install; the tools stay on the server, so updating the
//! server updates them for everyone.

use crate::api::{ApiError, ApiResult};
use crate::app::Caller;
use axum::extract::Query;
use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

const RELAY: &str = include_str!("../../mcpb/server/index.js");
const ICON: &[u8] = include_bytes!("../../mcpb/icon.png");

#[derive(Debug, Deserialize)]
pub struct ExtensionQuery {
  /// The instance's address as the browser reaches it (`location.origin`):
  /// the computer that installs the extension reaches it the same way.
  #[serde(default)]
  pub base: Option<String>,
}

pub async fn desktop_extension(caller: Caller, headers: HeaderMap, Query(query): Query<ExtensionQuery>) -> ApiResult<Response> {
  let base = match query.base {
    Some(base) => checked_base(&base)?,
    None => base_from_headers(&headers),
  };
  let bytes = bundle(&format!("{base}/mcp"), caller.key.as_deref())?;
  Ok((
    [
      (header::CONTENT_TYPE, "application/zip".to_string()),
      (header::CONTENT_DISPOSITION, "attachment; filename=\"sagex-hours.mcpb\"".to_string()),
    ],
    bytes,
  )
    .into_response())
}

fn checked_base(raw: &str) -> ApiResult<String> {
  let base = raw.trim().trim_end_matches('/');
  let scheme_ok = base.starts_with("http://") || base.starts_with("https://");
  if !scheme_ok || base.chars().any(|c| c.is_whitespace() || c == '"' || c == '\\') {
    return Err(ApiError::bad_request(format!("invalid base address: {raw}")));
  }
  Ok(base.to_string())
}

/// Behind a reverse proxy, the address it was reached at.
fn base_from_headers(headers: &HeaderMap) -> String {
  let value = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(|v| v.split(',').next().unwrap_or("").trim().to_string());
  let scheme = value("x-forwarded-proto").filter(|s| !s.is_empty()).unwrap_or_else(|| "http".into());
  let host = value("x-forwarded-host").or_else(|| value("host")).filter(|h| !h.is_empty()).unwrap_or_else(|| "localhost:8080".into());
  format!("{scheme}://{host}")
}

/// The extension's manifest (MCPB 0.3).
pub fn manifest(mcp_url: &str, key: Option<&str>) -> Value {
  json!({
    "manifest_version": "0.3",
    "name": "sagex-hours",
    "display_name": "SageX Hours",
    "version": env!("CARGO_PKG_VERSION"),
    "description": "Enter and check your SageX hours by talking to Claude.",
    "long_description": format!(
      "Connects Claude to your hours on the sagex-hours instance at {mcp_url}. Claude can add, move and copy entries, check a month before the SageX export and manage your projects, activities and tasks; every change shows at once in the app. The extension holds your personal key: don't pass it on."
    ),
    "author": { "name": "sagex-hours" },
    "icon": "icon.png",
    "server": {
      "type": "node",
      "entry_point": "server/index.js",
      "mcp_config": {
        "command": "node",
        "args": ["${__dirname}/server/index.js"],
        "env": { "SAGEX_URL": mcp_url, "SAGEX_TOKEN": key.unwrap_or("") },
      },
    },
    "tools": crate::mcp::tools().iter().map(|t| json!({ "name": t.name, "description": t.description })).collect::<Vec<_>>(),
    "compatibility": { "platforms": ["darwin", "win32", "linux"], "runtimes": { "node": ">=18.0.0" } },
  })
}

pub fn bundle(mcp_url: &str, key: Option<&str>) -> anyhow::Result<Vec<u8>> {
  let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
  let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
  let manifest = serde_json::to_string_pretty(&manifest(mcp_url, key))?;
  for (name, content) in [("manifest.json", manifest.as_bytes()), ("server/index.js", RELAY.as_bytes()), ("icon.png", ICON)] {
    writer.start_file(name, options)?;
    writer.write_all(content)?;
  }
  Ok(writer.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Read;

  #[test]
  fn the_bundle_carries_the_address_the_key_and_the_relay() {
    let bytes = bundle("http://server:8080/mcp", Some("k123")).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut manifest = String::new();
    archive.by_name("manifest.json").unwrap().read_to_string(&mut manifest).unwrap();
    let manifest: Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["server"]["mcp_config"]["env"]["SAGEX_URL"], "http://server:8080/mcp");
    assert_eq!(manifest["server"]["mcp_config"]["env"]["SAGEX_TOKEN"], "k123");
    assert_eq!(manifest["server"]["type"], "node");
    assert!(manifest["tools"].as_array().unwrap().len() > 10);
    assert!(archive.by_name("server/index.js").is_ok() && archive.by_name("icon.png").is_ok());
  }

  #[test]
  fn the_base_address_is_checked_or_taken_from_the_proxy() {
    assert_eq!(checked_base("http://10.0.0.5:8099/").unwrap(), "http://10.0.0.5:8099");
    assert!(checked_base("javascript:alert(1)").is_err());
    assert!(checked_base("http://a b").is_err());
    let mut headers = HeaderMap::new();
    headers.insert("host", "internal:8080".parse().unwrap());
    assert_eq!(base_from_headers(&headers), "http://internal:8080");
    headers.insert("x-forwarded-proto", "https".parse().unwrap());
    headers.insert("x-forwarded-host", "hours.lab.example".parse().unwrap());
    assert_eq!(base_from_headers(&headers), "https://hours.lab.example");
  }
}
