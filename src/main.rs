mod api;
mod app;
mod export;
mod import_ron;
mod model;
mod parse;
mod settings;
mod slug;
mod store;
mod toggl;
mod validate;
mod watch;
mod web;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sagex-hours", version, about = "Saisie des heures SageX : base JSON en clair, API, UI web, export xlsx")]
struct Cli {
  /// Dossier de la base (défaut : ./data, ou $SAGEX_DATA_DIR).
  #[arg(long, global = true, env = "SAGEX_DATA_DIR", default_value = "data")]
  data: PathBuf,

  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// Démarre le serveur web et l'API.
  Serve {
    #[arg(long, env = "SAGEX_PORT", default_value_t = 8080)]
    port: u16,
    #[arg(long, env = "SAGEX_HOST", default_value = "0.0.0.0")]
    host: String,
  },
  /// Rapatrie une période depuis Toggl Track.
  ImportToggl {
    /// Début de la période (AAAA-MM-JJ), ou un mois entier avec --month.
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    to: Option<String>,
    /// Raccourci pour un mois entier (AAAA-MM).
    #[arg(long)]
    month: Option<String>,
    /// Affiche ce qui serait importé sans rien écrire.
    #[arg(long)]
    dry_run: bool,
  },
  /// Exporte un mois au format xlsx pour SageX.
  Export {
    /// Mois à exporter (AAAA-MM).
    month: String,
    /// Fichier de sortie (défaut : <mois>.xlsx dans le dossier courant).
    #[arg(long, short)]
    out: Option<PathBuf>,
    /// Exporte malgré les erreurs signalées.
    #[arg(long)]
    force: bool,
  },
  /// Convertit un sagex-rs.ron en catalogue (projets, activités, tâches).
  ImportRon {
    /// Chemin du fichier sagex-rs.ron.
    file: PathBuf,
    /// Affiche le résultat sans rien écrire.
    #[arg(long)]
    dry_run: bool,
    /// Écrase un catalogue existant.
    #[arg(long)]
    force: bool,
  },
}

#[tokio::main]
async fn main() -> Result<()> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("SAGEX_LOG").unwrap_or_else(|_| "sagex_hours=info,tower_http=warn".into()))
    .with_target(false)
    .init();

  let cli = Cli::parse();
  match cli.command {
    Command::Serve { port, host } => serve(&cli.data, &host, port).await,
    Command::ImportRon { file, dry_run, force } => import_ron_command(&cli.data, &file, dry_run, force),
    Command::ImportToggl { from, to, month, dry_run } => import_toggl_command(&cli.data, from, to, month, dry_run).await,
    Command::Export { month, out, force } => export_command(&cli.data, &month, out, force),
  }
}

async fn import_toggl_command(
  data: &PathBuf,
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
    _ => anyhow::bail!("précise --month AAAA-MM, ou --from et --to"),
  };

  println!("Import Toggl du {from} au {to}…");
  let store = store::Store::open(data)?;
  let summary = toggl::run(&store, from, to, dry_run).await?;
  summary.print(dry_run);
  Ok(())
}

fn export_command(data: &PathBuf, month: &str, out: Option<PathBuf>, force: bool) -> Result<()> {
  let month: model::YearMonth = month.parse()?;
  let store = store::Store::open(data)?;
  let settings = store.settings()?;
  let catalog = store.catalog()?;
  let entries = store.month(month)?.entries;

  let report = validate::report(month, &entries, &catalog, &settings);
  println!(
    "{month} : {} saisies, {} vers SageX, {} attendues.",
    export::format_hours(report.total_minutes),
    export::format_hours(report.exported_minutes),
    export::format_hours(report.expected_minutes)
  );
  for issue in &report.issues {
    println!("  {:?} {}", issue.severity, issue.message);
  }
  if report.has_errors() && !force {
    anyhow::bail!("des erreurs bloquent l'export — corrige-les ou relance avec --force");
  }

  let built = export::build(&entries, &catalog, &settings, Some(month));
  if built.rows.is_empty() {
    anyhow::bail!("aucune ligne à exporter pour {month}");
  }
  let bytes = export::to_xlsx(&built.rows, &settings)?;
  let path = out.unwrap_or_else(|| PathBuf::from(format!("sagex_{month}.xlsx")));
  std::fs::write(&path, bytes).with_context(|| format!("écriture de {}", path.display()))?;
  println!("Écrit : {} ({} ligne(s))", path.display(), built.rows.len());
  println!("Rappel : ré-enregistrer en .xls depuis Excel avant l'import dans SageX.");
  Ok(())
}

async fn serve(data: &PathBuf, host: &str, port: u16) -> Result<()> {
  let store = store::Store::open(data)?;
  let auth_token = std::env::var("SAGEX_TOKEN").ok().filter(|t| !t.is_empty());
  let state = app::AppState::new(store, auth_token);

  // Gardée en vie le temps du processus : la surveillance s'arrête avec elle.
  let _watcher = watch::spawn(state.clone())?;

  let router = api::router(state.clone()).merge(web::router()).fallback(web::not_found);

  let address = format!("{host}:{port}");
  let listener = tokio::net::TcpListener::bind(&address).await.with_context(|| format!("écoute sur {address}"))?;

  println!("sagex-hours");
  println!("  base      : {}", state.store.root().canonicalize().unwrap_or_else(|_| state.store.root().to_path_buf()).display());
  println!("  interface : http://{}", if host == "0.0.0.0" { format!("localhost:{port}") } else { address.clone() });
  println!("  accès     : {}", if state.auth_token.is_some() { "jeton requis (SAGEX_TOKEN)" } else { "ouvert (aucun jeton configuré)" });

  axum::serve(listener, router).with_graceful_shutdown(shutdown()).await?;
  Ok(())
}

async fn shutdown() {
  let _ = tokio::signal::ctrl_c().await;
  println!("\narrêt.");
}

fn import_ron_command(data: &PathBuf, file: &PathBuf, dry_run: bool, force: bool) -> Result<()> {
  let source = std::fs::read_to_string(file).with_context(|| format!("lecture de {}", file.display()))?;
  let store = store::Store::open(data)?;
  let mut settings = store.settings()?;
  let (catalog, report) = import_ron::convert(&source, &mut settings)?;

  let existing = store.catalog()?;
  let occupied = !existing.projects.is_empty() || !existing.activities.is_empty() || !existing.presets.is_empty();
  if occupied && !force && !dry_run {
    anyhow::bail!(
      "{} contient déjà un catalogue ({} projets, {} activités, {} tâches) — relance avec --force pour l'écraser",
      store.catalog_path().display(),
      existing.projects.len(),
      existing.activities.len(),
      existing.presets.len()
    );
  }

  report.print();
  if dry_run {
    println!("\n--dry-run : rien n'a été écrit.");
    println!("{}", serde_json::to_string_pretty(&catalog)?);
    return Ok(());
  }

  store.save_catalog(&catalog)?;
  store.save_settings(&settings)?;
  println!("\nÉcrit : {}", store.catalog_path().display());
  println!("Écrit : {}", store.settings_path().display());
  if settings.toggl.api_token.is_some() {
    println!("Le token Toggl a été repris dans settings.json (dossier de données, non versionné).");
  }
  Ok(())
}
