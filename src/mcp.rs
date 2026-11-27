//! The MCP endpoint, `POST /mcp` (Streamable HTTP, stateless, JSON answers):
//! lets Claude enter and check hours.
//!
//! Every tool is a call to the HTTP API, made in-process with the caller's
//! key, so the tools and the API share their checks and their messages.
//! Claude Code connects to it directly; Claude Desktop through the extension
//! of `api/claude.rs`, a relay run by its own Node.js.

use crate::api;
use crate::app::AppState;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Map, Value};
use tower::ServiceExt;

/// The versions of the protocol this endpoint speaks, newest first.
const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "\
sagex-hours keeps the user's SageX hours (HES-SO): the entries of each day, and the monthly .xlsx that SageX imports.
- Start with get_overview: the catalogue lists projects (SageX projects), activities (one list valid for every project) and tasks (shortcuts: a project + an activity + a default comment). References accept the identifier, the SageX number or the name, even partial. Never invent a project or activity number: if one is missing, ask, or add it with add_catalog_item once the user gives it.
- An entry needs a date, a project and an activity (or a task, which fills both and the comment) and a duration. Durations: 1h30, 1:30, 90 (minutes), 90min, 1.5h. Dates: 2026-09-16, 16.09.2026, today, yesterday. Times are optional: an entry without them sits in the day's untimed band, which is fine.
- Rules: a 100 % position is 8h12 a day (SageX 2026 directive) and the Valais public holidays are computed; SageX's comment column holds 1000 characters; a monthly row is dated the last day of the month; the previous month's hours are entered during the first week of the current month; a project whose SageX number is 0, or not exportable, never goes to SageX.
- Before suggesting an export, run month_report: issues of severity \"error\" block it. The .xlsx is downloaded from the app's Month view, then re-saved as .xls in Excel before the import into SageX (Heures → Transfert des heures).
- clear_entries and import_toggl only count by default (dry_run); run them with dry_run false once the user agrees with what they would do.
- Whatever you change shows at once in the app the user has open.";

pub fn router(state: AppState) -> Router {
  Router::new().route("/mcp", post(handle).get(not_allowed).delete(not_allowed)).with_state(state)
}

/// No server-initiated stream and no session to end: only POST is served.
async fn not_allowed() -> StatusCode {
  StatusCode::METHOD_NOT_ALLOWED
}

async fn handle(State(state): State<AppState>, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
  let key = api::presented_key(&uri, &headers);
  if state.resolve(key.as_deref()).is_none() {
    return (StatusCode::UNAUTHORIZED, Json(rpc_error(Value::Null, -32001, "invalid or missing access key"))).into_response();
  }
  let parsed: Value = match serde_json::from_slice(&body) {
    Ok(value) => value,
    Err(err) => return (StatusCode::BAD_REQUEST, Json(rpc_error(Value::Null, -32700, &format!("parse error: {err}")))).into_response(),
  };
  let (messages, batch) = match parsed {
    Value::Array(items) => (items, true),
    other => (vec![other], false),
  };

  let mut replies = Vec::new();
  for message in messages {
    if let Some(reply) = answer(&state, key.as_deref(), message).await {
      replies.push(reply);
    }
  }
  // Notifications and responses call for no answer.
  if replies.is_empty() {
    return StatusCode::ACCEPTED.into_response();
  }
  Json(if batch { Value::Array(replies) } else { replies.remove(0) }).into_response()
}

async fn answer(state: &AppState, key: Option<&str>, message: Value) -> Option<Value> {
  let id = message.get("id").cloned()?;
  let method = message.get("method")?.as_str()?.to_string();
  let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

  let result = match method.as_str() {
    "initialize" => Ok(initialize(&params)),
    "ping" => Ok(json!({})),
    "tools/list" => Ok(json!({ "tools": tools().iter().map(Tool::listing).collect::<Vec<_>>() })),
    "tools/call" => call_tool(state, key, &params).await,
    // Some clients ask even when the capabilities do not offer them.
    "resources/list" => Ok(json!({ "resources": [] })),
    "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
    "prompts/list" => Ok(json!({ "prompts": [] })),
    other => Err((-32601, format!("method not found: {other}"))),
  };
  Some(match result {
    Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    Err((code, text)) => rpc_error(id, code, &text),
  })
}

fn initialize(params: &Value) -> Value {
  let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
  let version = PROTOCOL_VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(PROTOCOL_VERSIONS[1]);
  json!({
    "protocolVersion": version,
    "capabilities": { "tools": { "listChanged": false } },
    "serverInfo": { "name": "sagex-hours", "title": "SageX Hours", "version": env!("CARGO_PKG_VERSION") },
    "instructions": INSTRUCTIONS,
  })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
  json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Runs a tool through the API, as the caller.
async fn call_tool(state: &AppState, key: Option<&str>, params: &Value) -> Result<Value, (i64, String)> {
  let name = params.get("name").and_then(Value::as_str).unwrap_or("");
  let tool = tools().into_iter().find(|t| t.name == name).ok_or_else(|| (-32602, format!("unknown tool: {name}")))?;
  let arguments = match params.get("arguments") {
    Some(Value::Object(map)) => map.clone(),
    None | Some(Value::Null) => Map::new(),
    Some(_) => return Err((-32602, "`arguments` must be an object".to_string())),
  };

  let call = match (tool.route)(&arguments) {
    Ok(call) => call,
    Err(problem) => return Ok(tool_result(problem, true)),
  };

  let mut request = Request::builder()
    .method(call.method)
    .uri(call.path)
    .header("content-type", "application/json")
    .header("accept-language", "en");
  if let Some(key) = key {
    request = request.header("x-auth-token", key);
  }
  let body = call.body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty);
  let request = request.body(body).map_err(|e| (-32603, format!("internal error: {e}")))?;

  let response = match api::router(state.clone()).oneshot(request).await {
    Ok(response) => response,
    Err(never) => match never {},
  };
  let status = response.status();
  let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024).await.map_err(|e| (-32603, format!("internal error: {e}")))?;
  let text = String::from_utf8_lossy(&bytes).to_string();
  if status.is_success() {
    Ok(tool_result(text, false))
  } else {
    // The API's message, without its JSON wrapping.
    let message = serde_json::from_str::<Value>(&text)
      .ok()
      .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
      .unwrap_or(text);
    Ok(tool_result(format!("{status}: {message}"), true))
  }
}

fn tool_result(text: String, is_error: bool) -> Value {
  json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

// ---------------------------------------------------------------------------
// The tools
// ---------------------------------------------------------------------------

pub struct Tool {
  pub name: &'static str,
  pub title: &'static str,
  pub description: &'static str,
  schema: Value,
  read_only: bool,
  route: fn(&Map<String, Value>) -> Result<Call, String>,
}

struct Call {
  method: Method,
  path: String,
  body: Option<Value>,
}

impl Tool {
  fn listing(&self) -> Value {
    json!({
      "name": self.name,
      "title": self.title,
      "description": self.description,
      "inputSchema": self.schema,
      "annotations": { "readOnlyHint": self.read_only },
    })
  }
}

fn get(path: String) -> Result<Call, String> {
  Ok(Call { method: Method::GET, path, body: None })
}

fn send(method: Method, path: String, body: Value) -> Result<Call, String> {
  Ok(Call { method, path, body: Some(body) })
}

fn text(args: &Map<String, Value>, name: &str) -> Option<String> {
  match args.get(name)? {
    Value::String(s) => Some(s.clone()),
    Value::Null => None,
    other => Some(other.to_string()),
  }
}

fn required(args: &Map<String, Value>, name: &str) -> Result<String, String> {
  text(args, name).filter(|s| !s.trim().is_empty()).ok_or_else(|| format!("`{name}` is required"))
}

/// The arguments without the routing ones (they go in the path).
fn without(args: &Map<String, Value>, names: &[&str]) -> Value {
  Value::Object(args.iter().filter(|(k, _)| !names.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// `?a=1&b=2` from the arguments that are set.
fn query(pairs: &[(&str, Option<String>)]) -> String {
  let set: Vec<String> = pairs.iter().filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}={}", encode(v)))).collect();
  if set.is_empty() {
    String::new()
  } else {
    format!("?{}", set.join("&"))
  }
}

fn encode(value: &str) -> String {
  value
    .bytes()
    .map(|b| match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
      other => format!("%{other:02X}"),
    })
    .collect()
}

/// The list kinds of the catalogue, checked before they reach a path.
fn kind(args: &Map<String, Value>) -> Result<String, String> {
  let kind = required(args, "kind")?;
  match kind.as_str() {
    "projects" | "activities" | "presets" => Ok(kind),
    other => Err(format!("unknown kind `{other}`: use projects, activities or presets")),
  }
}

/// `dry_run` defaults to true for the tools that delete or import.
fn with_dry_run_default(args: &Map<String, Value>) -> Value {
  let mut body = without(args, &[]);
  if let Value::Object(map) = &mut body {
    map.entry("dry_run").or_insert(Value::Bool(true));
  }
  body
}

fn entry_properties() -> Value {
  json!({
    "date": { "type": "string", "description": "2026-09-16, 16.09.2026, today, yesterday…" },
    "preset": { "type": "string", "description": "A task (identifier or label): fills project, activity and comment." },
    "project": { "type": ["string", "integer"], "description": "Project: identifier, SageX number or name (even partial)." },
    "activity": { "type": ["string", "integer"], "description": "Activity: identifier, SageX number or name (even partial)." },
    "comment": { "type": "string", "description": "What was done; goes to SageX's comment column." },
    "start": { "type": ["string", "null"], "description": "HH:MM, optional." },
    "end": { "type": ["string", "null"], "description": "HH:MM, optional." },
    "duration": { "type": "string", "description": "1h30, 1:30, 90, 90min, 1.5h." },
    "minutes": { "type": "integer" },
    "hours": { "type": "number" },
  })
}

fn range_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "from": { "type": "string", "description": "First day (default: this week's Monday)." },
      "to": { "type": "string", "description": "Last day, inclusive (default: six days after `from`)." },
    },
  })
}

fn catalog_item_properties() -> Value {
  json!({
    "kind": { "type": "string", "enum": ["projects", "activities", "presets"] },
    "name": { "type": "string", "description": "Projects and activities: the name." },
    "label": { "type": "string", "description": "Tasks: the short label." },
    "sagex_number": { "type": "integer", "description": "Projects and activities: the number in SageX (0 for an internal project)." },
    "project": { "type": ["string", "integer"], "description": "Tasks: their project." },
    "activity": { "type": ["string", "integer"], "description": "Tasks: their activity." },
    "comment": { "type": "string", "description": "Tasks: the default comment (else the label)." },
    "color": { "type": "string", "description": "#rrggbb" },
    "exportable": { "type": "boolean", "description": "Projects: false keeps it out of SageX (holidays, time off)." },
    "archived": { "type": "boolean" },
    "note": { "type": "string" },
  })
}

pub fn tools() -> Vec<Tool> {
  vec![
    Tool {
      name: "get_overview",
      title: "Overview of a period",
      description: "The catalogue (projects, activities and tasks, with the identifiers to use), the settings, the running timer, the public holidays and the entries of a period (default: the current week).",
      schema: range_schema(),
      read_only: true,
      route: |a| get(format!("/api/state{}", query(&[("from", text(a, "from")), ("to", text(a, "to"))]))),
    },
    Tool {
      name: "list_entries",
      title: "List entries",
      description: "The entries between two dates (inclusive), sorted by date and time.",
      schema: range_schema(),
      read_only: true,
      route: |a| get(format!("/api/entries{}", query(&[("from", text(a, "from")), ("to", text(a, "to"))]))),
    },
    Tool {
      name: "add_entries",
      title: "Add entries",
      description: "Adds one or more entries. Each needs a date, a project and an activity (or a task, `preset`), and a duration: `duration`, `minutes`, `hours`, or `start` + `end`. Without times, an entry goes in the day's untimed band.",
      schema: json!({
        "type": "object",
        "properties": { "entries": { "type": "array", "minItems": 1, "items": { "type": "object", "properties": entry_properties() } } },
        "required": ["entries"],
      }),
      read_only: false,
      route: |a| match a.get("entries") {
        Some(entries @ Value::Array(_)) => send(Method::POST, "/api/entries".into(), entries.clone()),
        _ => Err("`entries` must be an array of entries".into()),
      },
    },
    Tool {
      name: "update_entry",
      title: "Change an entry",
      description: "Changes an entry: only the fields given change. `start` or `end` set to null clear the time; the duration then stays.",
      schema: {
        let mut properties = entry_properties();
        properties["id"] = json!({ "type": "string" });
        json!({ "type": "object", "properties": properties, "required": ["id"] })
      },
      read_only: false,
      route: |a| send(Method::PATCH, format!("/api/entries/{}", encode(&required(a, "id")?)), without(a, &["id"])),
    },
    Tool {
      name: "delete_entry",
      title: "Delete an entry",
      description: "Deletes one entry by its identifier.",
      schema: json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] }),
      read_only: false,
      route: |a| Ok(Call { method: Method::DELETE, path: format!("/api/entries/{}", encode(&required(a, "id")?)), body: None }),
    },
    Tool {
      name: "copy_entries",
      title: "Copy a day or a week",
      description: "Copies the entries of a day to other days (mode day), or of a week to other weeks (mode week; source and target: any day of those weeks).",
      schema: json!({
        "type": "object",
        "properties": {
          "mode": { "type": "string", "enum": ["day", "week"] },
          "source": { "type": "string" },
          "target": { "type": "string" },
          "targets": { "type": "array", "items": { "type": "string" } },
          "replace": { "type": "boolean", "description": "Empty the target first." },
          "skip_weekend": { "type": "boolean", "description": "Week mode: leave Saturday and Sunday out." },
          "comment": { "type": "string", "description": "Replaces the comment of every copy." },
        },
        "required": ["source"],
      }),
      read_only: false,
      route: |a| send(Method::POST, "/api/entries/copy".into(), without(a, &[])),
    },
    Tool {
      name: "clear_entries",
      title: "Clear a period",
      description: "Deletes the entries of a period, or only those of one project. Only counts unless dry_run is false.",
      schema: json!({
        "type": "object",
        "properties": {
          "from": { "type": "string" },
          "to": { "type": "string", "description": "Default: same day as `from`." },
          "project": { "type": ["string", "integer"] },
          "dry_run": { "type": "boolean", "default": true },
        },
        "required": ["from"],
      }),
      read_only: false,
      route: |a| send(Method::POST, "/api/entries/clear".into(), with_dry_run_default(a)),
    },
    Tool {
      name: "month_report",
      title: "Month report",
      description: "A month's totals (entered, to SageX, expected, gap), its checks (`issues`, errors block the export) and the preview of the rows that will go to SageX.",
      schema: json!({
        "type": "object",
        "properties": {
          "month": { "type": "string", "description": "YYYY-MM" },
          "aggregate": { "type": "string", "enum": ["month", "day", "none"] },
          "comment_style": { "type": "string", "enum": ["auto", "detailed", "compact"] },
        },
        "required": ["month"],
      }),
      read_only: true,
      route: |a| {
        get(format!(
          "/api/month/{}/report{}",
          encode(&required(a, "month")?),
          query(&[("aggregate", text(a, "aggregate")), ("comment_style", text(a, "comment_style"))])
        ))
      },
    },
    Tool {
      name: "get_catalog",
      title: "Catalogue",
      description: "Projects, activities and tasks, with their identifiers and SageX numbers.",
      schema: json!({ "type": "object", "properties": {} }),
      read_only: true,
      route: |_| get("/api/catalog".into()),
    },
    Tool {
      name: "add_catalog_item",
      title: "Add a project, an activity or a task",
      description: "Adds to one of the catalogue's lists. A SageX number already used is refused.",
      schema: json!({ "type": "object", "properties": catalog_item_properties(), "required": ["kind"] }),
      read_only: false,
      route: |a| send(Method::POST, format!("/api/catalog/{}", kind(a)?), without(a, &["kind"])),
    },
    Tool {
      name: "update_catalog_item",
      title: "Change a catalogue item",
      description: "Changes a project, an activity or a task: only the fields given change. Archive rather than delete what entries still use.",
      schema: {
        let mut properties = catalog_item_properties();
        properties["id"] = json!({ "type": "string" });
        json!({ "type": "object", "properties": properties, "required": ["kind", "id"] })
      },
      read_only: false,
      route: |a| send(Method::PATCH, format!("/api/catalog/{}/{}", kind(a)?, encode(&required(a, "id")?)), without(a, &["kind", "id"])),
    },
    Tool {
      name: "delete_catalog_item",
      title: "Delete a catalogue item",
      description: "Deletes a project, an activity or a task. Refused while entries use it, unless force is true (they would point to nothing).",
      schema: json!({
        "type": "object",
        "properties": {
          "kind": { "type": "string", "enum": ["projects", "activities", "presets"] },
          "id": { "type": "string" },
          "force": { "type": "boolean" },
        },
        "required": ["kind", "id"],
      }),
      read_only: false,
      route: |a| {
        let force = a.get("force").and_then(Value::as_bool).unwrap_or(false);
        Ok(Call { method: Method::DELETE, path: format!("/api/catalog/{}/{}?force={force}", kind(a)?, encode(&required(a, "id")?)), body: None })
      },
    },
    Tool {
      name: "get_settings",
      title: "Settings",
      description: "SageX identity (employee, person and school numbers), activity rate, daily hours, extra holidays, export options.",
      schema: json!({ "type": "object", "properties": {} }),
      read_only: true,
      route: |_| get("/api/settings".into()),
    },
    Tool {
      name: "update_settings",
      title: "Change settings",
      description: "Merges `patch` into the settings, e.g. {\"activity_rate\": 0.8} or {\"extra_holidays\": [\"2026-12-24\"]}.",
      schema: json!({ "type": "object", "properties": { "patch": { "type": "object" } }, "required": ["patch"] }),
      read_only: false,
      route: |a| match a.get("patch") {
        Some(patch @ Value::Object(_)) => send(Method::PATCH, "/api/settings".into(), patch.clone()),
        _ => Err("`patch` must be an object".into()),
      },
    },
    Tool {
      name: "start_timer",
      title: "Start the timer",
      description: "Starts the timer on a task or a project + activity; `started_at` (HH:MM) if it began earlier today.",
      schema: json!({
        "type": "object",
        "properties": {
          "preset": { "type": "string" },
          "project": { "type": ["string", "integer"] },
          "activity": { "type": ["string", "integer"] },
          "comment": { "type": "string" },
          "started_at": { "type": "string" },
        },
      }),
      read_only: false,
      route: |a| send(Method::POST, "/api/timer/start".into(), without(a, &[])),
    },
    Tool {
      name: "stop_timer",
      title: "Stop the timer",
      description: "Stops the timer and saves the entry (rounded to `round_to` minutes if given), or throws it away with discard.",
      schema: json!({
        "type": "object",
        "properties": { "comment": { "type": "string" }, "round_to": { "type": "integer" }, "discard": { "type": "boolean" } },
      }),
      read_only: false,
      route: |a| send(Method::POST, "/api/timer/stop".into(), without(a, &[])),
    },
    Tool {
      name: "import_toggl",
      title: "Import from Toggl",
      description: "Brings a month (or from/to) back from Toggl Track. Only counts unless dry_run is false; the summary lists the Toggl client/project pairs without a matching task, to create first. Entries already imported are recognised.",
      schema: json!({
        "type": "object",
        "properties": {
          "month": { "type": "string", "description": "YYYY-MM" },
          "from": { "type": "string" },
          "to": { "type": "string" },
          "dry_run": { "type": "boolean", "default": true },
        },
      }),
      read_only: false,
      route: |a| send(Method::POST, "/api/import/toggl".into(), with_dry_run_default(a)),
    },
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  async fn post(state: &AppState, key: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut request = Request::builder().method(Method::POST).uri("/mcp").header("content-type", "application/json");
    if let Some(key) = key {
      request = request.header("authorization", format!("Bearer {key}"));
    }
    let response = router(state.clone()).oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() })
  }

  fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
  }

  fn seeded() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path(), Some("secret".into())).unwrap();
    let store = Space::open(dir.path()).unwrap().store;
    let mut catalog = store.catalog().unwrap();
    catalog.projects.push(crate::model::Project {
      id: "spl-base".into(),
      sagex_number: 89403,
      name: "SPL Base".into(),
      color: "#000".into(),
      archived: false,
      exportable: true,
      note: None,
    });
    catalog.activities.push(crate::model::Activity { id: "ra-d".into(), sagex_number: 29, name: "Ra&D".into(), archived: false, note: None });
    store.save_catalog(&catalog).unwrap();
    (dir, state)
  }

  use crate::app::Space;

  #[tokio::test]
  async fn handshake_and_tool_list() {
    let (_dir, state) = seeded();
    let (status, reply) = post(&state, Some("secret"), json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
    assert!(reply["result"]["instructions"].as_str().unwrap().contains("8h12"));

    let (status, reply) = post(&state, Some("secret"), json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(reply.is_null());

    let (_, reply) = post(&state, Some("secret"), json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
    let names: Vec<&str> = reply["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"add_entries") && names.contains(&"month_report"), "{names:?}");

    let (status, _) = post(&state, Some("wrong"), json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
  }

  #[tokio::test]
  async fn tools_go_through_the_api() {
    let (_dir, state) = seeded();
    let entries = json!({ "entries": [{ "date": "2026-09-16", "project": "SPL", "activity": 29, "duration": "1h30", "comment": "Model" }] });
    let (_, reply) = post(&state, Some("secret"), call(1, "add_entries", entries)).await;
    assert_eq!(reply["result"]["isError"], false, "{reply}");

    let (_, reply) = post(&state, Some("secret"), call(2, "list_entries", json!({ "from": "2026-09-14", "to": "2026-09-20" }))).await;
    let listed: Value = serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(listed[0]["minutes"], 90);
    assert_eq!(listed[0]["project_id"], "spl-base");

    // An API refusal is a tool error, in English, with its reason.
    let (_, reply) = post(&state, Some("secret"), call(3, "add_entries", json!({ "entries": [{ "date": "2026-09-16", "project": "SPL" }] }))).await;
    assert_eq!(reply["result"]["isError"], true);
    assert!(reply["result"]["content"][0]["text"].as_str().unwrap().contains("missing activity"), "{reply}");

    // Clearing only counts unless asked.
    let (_, reply) = post(&state, Some("secret"), call(4, "clear_entries", json!({ "from": "2026-09-16" }))).await;
    assert!(reply["result"]["content"][0]["text"].as_str().unwrap().contains("would_delete"), "{reply}");

    let (_, reply) = post(&state, Some("secret"), call(5, "nope", json!({}))).await;
    assert_eq!(reply["error"]["code"], -32602);
  }
}
