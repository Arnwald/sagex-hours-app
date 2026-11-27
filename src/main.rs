#[macro_use]
mod i18n;

mod api;
mod app;
mod export;
mod import_ron;
mod mcp;
mod model;
mod parse;
mod settings;
mod slug;
mod store;
mod toggl;
mod users;
mod validate;
mod watch;
mod web;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "sagex-hours", version, about = "SageX hours: a plain JSON base, an HTTP API and MCP endpoint, a web interface, the xlsx export")]
struct Cli {
  /// Data folder (default: ./data, or $SAGEX_DATA_DIR).
  #[arg(long, global = true, env = "SAGEX_DATA_DIR", default_value = "data")]
  data: PathBuf,

  /// With several users: whose base the command works on.
  #[arg(long, global = true, env = "SAGEX_USER")]
  user: Option<String>,

  /// Language of the messages that come from the checks and the export (en, fr).
  #[arg(long, global = true, env = "SAGEX_LANG", default_value = "en")]
  lang: String,

  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// Starts the web server, the API and the MCP endpoint.
  Serve {
    #[arg(long, env = "SAGEX_PORT", default_value_t = 8080)]
    port: u16,
    #[arg(long, env = "SAGEX_HOST", default_value = "0.0.0.0")]
    host: String,
  },
  /// Brings a period back from Toggl Track.
  ImportToggl {
    /// First day of the period (YYYY-MM-DD), or a whole month with --month.
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    to: Option<String>,
    /// Shortcut for a whole month (YYYY-MM).
    #[arg(long)]
    month: Option<String>,
    /// Shows what would be imported without writing anything.
    #[arg(long)]
    dry_run: bool,
  },
  /// Exports a month as the xlsx SageX imports.
  Export {
    /// Month to export (YYYY-MM).
    month: String,
    /// Output file (default: sagex_<month>.xlsx in the current folder).
    #[arg(long, short)]
    out: Option<PathBuf>,
    /// Exports in spite of the errors reported.
    #[arg(long)]
    force: bool,
  },
  /// Turns a sagex-rs.ron into a catalogue (projects, activities, tasks).
  ImportRon {
    /// Path of the sagex-rs.ron file.
    file: PathBuf,
    /// Shows the result without writing anything.
    #[arg(long)]
    dry_run: bool,
    /// Overwrites an existing catalogue.
    #[arg(long)]
    force: bool,
  },
  /// Several people on one instance: each with a base and a personal key.
  Users {
    #[command(subcommand)]
    action: UsersAction,
  },
}

#[derive(Subcommand)]
enum UsersAction {
  /// Lists the users.
  List,
  /// Turns this single-user base into the first user's, an administrator;
  /// $SAGEX_TOKEN, if set, stays their key.
  Enable {
    /// The first user's name.
    #[arg(long)]
    name: String,
    /// Their identifier (default: made from the name).
    #[arg(long)]
    id: Option<String>,
  },
  /// Adds a user and prints their key.
  Add {
    /// Full name.
    name: String,
    /// Identifier, also their folder's name (default: made from the name).
    #[arg(long)]
    id: Option<String>,
    /// Can manage the users.
    #[arg(long)]
    admin: bool,
    /// User whose activities the new base starts from (default: the first administrator).
    #[arg(long)]
    from: Option<String>,
    /// Also start from that user's projects.
    #[arg(long)]
    with_projects: bool,
  },
  /// Gives a user a new key (the old one stops working).
  Key { id: String },
  /// Takes a user off the list; their folder stays.
  Remove { id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("SAGEX_LOG").unwrap_or_else(|_| "sagex_hours=info,tower_http=warn".into()))
    .with_target(false)
    .init();

  let cli = Cli::parse();
  let lang = i18n::Lang::from_code(&cli.lang).unwrap_or(i18n::Lang::En);
  i18n::scope(lang, run(cli)).await
}

async fn run(cli: Cli) -> Result<()> {
  let user = cli.user.as_deref();
  match cli.command {
    Command::Serve { port, host } => serve(&cli.data, &host, port).await,
    Command::ImportRon { file, dry_run, force } => import_ron_command(&open_base(&cli.data, user)?, &file, dry_run, force),
    Command::ImportToggl { from, to, month, dry_run } => import_toggl_command(&open_base(&cli.data, user)?, from, to, month, dry_run).await,
    Command::Export { month, out, force } => export_command(&open_base(&cli.data, user)?, &month, out, force),
    Command::Users { action } => users_command(&cli.data, action),
  }
}

/// The base a command works on: the data folder itself, or a user's folder.
fn open_base(data: &Path, user: Option<&str>) -> Result<store::Store> {
  match users::load(data)? {
    None => {
      if let Some(user) = user {
        bail!("--user {user}: this instance has a single user, drop --user");
      }
      store::Store::open(data)
    }
    Some(file) => {
      let id = match user {
        Some(id) => id.to_string(),
        None if file.users.len() == 1 => file.users[0].id.clone(),
        None => bail!(
          "several users: say whose base with --user ({})",
          file.users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>().join(", ")
        ),
      };
      if !file.users.iter().any(|u| u.id == id) {
        bail!("no user “{id}”");
      }
      store::Store::open(users::user_dir(data, &id))
    }
  }
}

fn token_from_env() -> Option<String> {
  std::env::var("SAGEX_TOKEN").ok().map(|t| t.split_whitespace().collect::<String>()).filter(|t| !t.is_empty())
}

async fn import_toggl_command(
  store: &store::Store,
  from: Option<String>,
  to: Option<String>,
  month: Option<String>,
  dry_run: bool,
) -> Result<()> {
  let (from, to) = match (&from, &to, &month) {
    (_, _, Some(month)) => {
      let month: model::YearMonth = month.parse()?;
      (month.first_day(), month.last_day())
    }
    (Some(from), Some(to), None) => (
      api::entries::parse_date_str(from).map_err(anyhow::Error::msg)?,
      api::entries::parse_date_str(to).map_err(anyhow::Error::msg)?,
    ),
    _ => bail!("give --month YYYY-MM, or --from and --to"),
  };

  println!("Toggl import from {from} to {to}…");
  let summary = toggl::run(store, from, to, dry_run).await?;
  summary.print(dry_run);
  Ok(())
}

fn export_command(store: &store::Store, month: &str, out: Option<PathBuf>, force: bool) -> Result<()> {
  let month: model::YearMonth = month.parse()?;
  let settings = store.settings()?;
  let catalog = store.catalog()?;
  let entries = store.month(month)?.entries;

  let report = validate::report(month, &entries, &catalog, &settings);
  println!(
    "{month}: {} entered, {} to SageX, {} expected.",
    export::format_hours(report.total_minutes),
    export::format_hours(report.exported_minutes),
    export::format_hours(report.expected_minutes)
  );
  for issue in &report.issues {
    println!("  {:?} {}", issue.severity, issue.message);
  }
  if report.has_errors() && !force {
    bail!("errors block the export — fix them or run again with --force");
  }

  let built = export::build(&entries, &catalog, &settings, Some(month));
  if built.rows.is_empty() {
    bail!("no row to export for {month}");
  }
  let bytes = export::to_xlsx(&built.rows, &settings)?;
  let path = out.unwrap_or_else(|| PathBuf::from(format!("sagex_{month}.xlsx")));
  std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
  println!("Written: {} ({} row(s))", path.display(), built.rows.len());
  println!("Reminder: re-save it as .xls from Excel before importing it into SageX.");
  Ok(())
}

fn users_command(data: &Path, action: UsersAction) -> Result<()> {
  let print_key = |user: &users::User, key: &str| {
    println!("  {} ({}){}", user.name, user.id, if user.admin { ", administrator" } else { "" });
    println!("  key: {key}");
    println!("  invitation: <address of the instance>/#key={key}");
  };
  match action {
    UsersAction::List => match users::load(data)? {
      None => println!("Single-user instance ({}).", data.display()),
      Some(file) => {
        for user in &file.users {
          println!("{:<20} {:<30} {}{}", user.id, user.name, user.created_at.date(), if user.admin { "  admin" } else { "" });
        }
      }
    },
    UsersAction::Enable { name, id } => {
      let (user, key) = users::enable(data, &name, id.as_deref(), token_from_env().as_deref())?;
      println!("Several users enabled; the existing base is now {}'s:", user.name);
      print_key(&user, &key);
    }
    UsersAction::Add { name, id, admin, from, with_projects } => {
      let file = users::load(data)?.context("single-user instance: run `sagex-hours users enable --name …` first")?;
      let source = match &from {
        Some(id) => file.users.iter().find(|u| &u.id == id).with_context(|| format!("no user “{id}”"))?,
        None => file.users.iter().find(|u| u.admin).or(file.users.first()).context("no user to start from")?,
      };
      let template = store::Store::open(users::user_dir(data, &source.id))?.catalog()?;
      let (user, key) = users::add(
        data,
        users::NewUser { name: &name, id: id.as_deref(), admin, template: Some(&template), with_projects },
      )?;
      println!("Added:");
      print_key(&user, &key);
    }
    UsersAction::Key { id } => {
      let (user, key) = users::renew_key(data, &id)?;
      println!("New key (the old one no longer works):");
      print_key(&user, &key);
    }
    UsersAction::Remove { id } => {
      let user = users::remove(data, &id)?;
      println!("Removed {} ({}); their folder stays: {}", user.name, user.id, users::user_dir(data, &user.id).display());
    }
  }
  Ok(())
}

async fn serve(data: &Path, host: &str, port: u16) -> Result<()> {
  let state = app::AppState::open(data, token_from_env())?;

  // Kept alive for the life of the process: watching stops with it.
  let _watcher = watch::spawn(state.clone())?;

  let router = api::router(state.clone()).merge(mcp::router(state.clone())).merge(web::router()).fallback(web::not_found);

  let address = format!("{host}:{port}");
  let listener = tokio::net::TcpListener::bind(&address).await.with_context(|| format!("listening on {address}"))?;

  let local = if host == "0.0.0.0" { format!("localhost:{port}") } else { address.clone() };
  println!("sagex-hours");
  println!("  data      : {}", state.root().display());
  println!("  interface : http://{local}");
  println!("  MCP       : http://{local}/mcp");
  println!(
    "  access    : {}",
    if state.is_multi_user() {
      format!("{} users, each with a personal key", state.users().len())
    } else if state.token_required() {
      "token required (SAGEX_TOKEN)".to_string()
    } else {
      "open (no token configured)".to_string()
    }
  );

  axum::serve(listener, router).with_graceful_shutdown(shutdown()).await?;
  Ok(())
}

async fn shutdown() {
  let _ = tokio::signal::ctrl_c().await;
  println!("\nstopped.");
}

fn import_ron_command(store: &store::Store, file: &Path, dry_run: bool, force: bool) -> Result<()> {
  let source = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
  let mut settings = store.settings()?;
  let (catalog, report) = import_ron::convert(&source, &mut settings)?;

  let existing = store.catalog()?;
  let occupied = !existing.projects.is_empty() || !existing.activities.is_empty() || !existing.presets.is_empty();
  if occupied && !force && !dry_run {
    bail!(
      "{} already holds a catalogue ({} projects, {} activities, {} tasks) — run again with --force to overwrite it",
      store.catalog_path().display(),
      existing.projects.len(),
      existing.activities.len(),
      existing.presets.len()
    );
  }

  report.print();
  if dry_run {
    println!("\n--dry-run: nothing was written.");
    println!("{}", serde_json::to_string_pretty(&catalog)?);
    return Ok(());
  }

  store.save_catalog(&catalog)?;
  store.save_settings(&settings)?;
  println!("\nWritten: {}", store.catalog_path().display());
  println!("Written: {}", store.settings_path().display());
  if settings.toggl.api_token.is_some() {
    println!("The Toggl token was copied into settings.json (data folder, not versioned).");
  }
  Ok(())
}
