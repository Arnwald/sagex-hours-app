//! Server-side translations: English and French.
//!
//! Messages are written in English where they are produced, with `tr!`; the
//! [`FR`] table maps each English template to its French version. The language
//! follows the request (`?lang=`, then `Accept-Language`), set for the whole
//! handler by [`scope`]; outside a request (the command line) it is English.
//!
//! Placeholders are written `{name}` and filled by name:
//! `tr!("entry {n}: {error}", n = 3, error = e)`.

use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
  En,
  Fr,
}

tokio::task_local! {
  static LANG: Lang;
}

impl Lang {
  /// The language of the request being handled, English outside one.
  pub fn current() -> Lang {
    LANG.try_with(|lang| *lang).unwrap_or(Lang::En)
  }

  /// `fr`, `fr-CH`, `EN_gb`…
  pub fn from_code(code: &str) -> Option<Lang> {
    let primary = code.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    match primary.as_str() {
      "en" => Some(Lang::En),
      "fr" => Some(Lang::Fr),
      _ => None,
    }
  }

  /// The preferred supported language of an `Accept-Language` header.
  pub fn from_accept_language(header: &str) -> Option<Lang> {
    let mut ranked: Vec<(f32, Lang)> = header
      .split(',')
      .filter_map(|part| {
        let mut pieces = part.split(';');
        let lang = Lang::from_code(pieces.next()?)?;
        let quality = pieces
          .find_map(|p| p.trim().strip_prefix("q="))
          .and_then(|q| q.trim().parse::<f32>().ok())
          .unwrap_or(1.0);
        Some((quality, lang))
      })
      .collect();
    // Stable: among equal weights, the first listed wins.
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ranked.first().map(|(_, lang)| *lang)
  }
}

/// Runs `future` with `lang` as the language of every message it produces.
pub async fn scope<F: Future>(lang: Lang, future: F) -> F::Output {
  LANG.scope(lang, future).await
}

/// The template of `key` in the current language.
pub fn translate(key: &'static str) -> &'static str {
  match Lang::current() {
    Lang::En => key,
    Lang::Fr => french().get(key).copied().unwrap_or(key),
  }
}

/// Fills the `{name}` placeholders of `template`, in one pass (a value that
/// itself contains braces is left alone).
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
  let mut out = String::with_capacity(template.len() + 16);
  let mut rest = template;
  while let Some(open) = rest.find('{') {
    out.push_str(&rest[..open]);
    let after = &rest[open + 1..];
    match after.find('}') {
      Some(close) if args.iter().any(|(name, _)| *name == &after[..close]) => {
        let (_, value) = args.iter().find(|(name, _)| *name == &after[..close]).expect("checked above");
        out.push_str(&value.to_string());
        rest = &after[close + 1..];
      }
      _ => {
        out.push('{');
        rest = after;
      }
    }
  }
  out.push_str(rest);
  out
}

/// `tr!("literal")` or `tr!("literal with {name}", name = value)`: the message in
/// the request's language, as a `String`.
#[macro_export]
macro_rules! tr {
  ($key:literal) => {
    $crate::i18n::translate($key).to_string()
  };
  ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
    $crate::i18n::fill($crate::i18n::translate($key), &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+])
  };
}

fn french() -> &'static HashMap<&'static str, &'static str> {
  static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
  MAP.get_or_init(|| FR.iter().copied().collect())
}

/// English template → French template. Every `tr!` key must appear here (a test
/// checks it), with the same placeholders.
pub const FR: &[(&str, &str)] = &[
  // -- access ---------------------------------------------------------------
  ("invalid or missing access key", "clé d'accès invalide ou absente"),
  ("this action is reserved for administrators", "cette action est réservée aux administrateurs"),
  // -- requests, dates and durations ---------------------------------------
  ("the range ends before it starts", "l'intervalle se termine avant de commencer"),
  ("date “{raw}” not understood — expected format YYYY-MM-DD", "date « {raw} » incomprise — format attendu AAAA-MM-JJ"),
  ("empty duration", "durée vide"),
  ("duration “{input}” is over 24 h", "durée « {input} » supérieure à 24 h"),
  ("duration “{input}” not understood — examples: 1h30, 1:30, 90, 90min, 1.5h", "durée « {input} » incomprise — exemples : 1h30, 1:30, 90, 90min, 1.5h"),
  ("zero duration", "durée nulle"),
  ("duration over 24 h", "durée supérieure à 24 h"),
  ("missing duration — give `duration` (“1h30”), `minutes`, `hours`, or `start` and `end`", "durée manquante — indique `duration` (« 1h30 »), `minutes`, `hours`, ou `start` et `end`"),
  ("the end time ({end}) is not after the start time ({start})", "l'heure de fin ({end}) n'est pas après l'heure de début ({start})"),
  ("missing project — give `preset` or `project`", "projet manquant — indique `preset` ou `project`"),
  ("missing activity — give `preset` or `activity`", "activité manquante — indique `preset` ou `activity`"),
  ("time “{raw}” not understood — expected HH:MM", "heure « {raw} » incomprise — format attendu HH:MM"),
  ("month “{raw}” not understood — expected YYYY-MM", "mois « {raw} » incompris — format attendu AAAA-MM"),
  // -- catalogue references -------------------------------------------------
  ("project", "projet"),
  ("activity", "activité"),
  ("task", "tâche"),
  ("{kind} not given", "{kind} : aucune référence donnée"),
  ("{kind} “{needle}” not found", "{kind} « {needle} » introuvable"),
  ("{kind} “{needle}” is ambiguous — candidates: {candidates}", "{kind} « {needle} » : plusieurs candidats — {candidates}"),
  // -- entries --------------------------------------------------------------
  ("no entry given", "aucune saisie fournie"),
  ("entry {n}: {error}", "saisie {n} : {error}"),
  ("entry “{id}” not found", "saisie « {id} » introuvable"),
  ("give `target` (or `targets`)", "précise `target` (ou `targets`)"),
  ("no entry on {date}", "aucune saisie le {date}"),
  ("no entry in the week of {date}", "aucune saisie dans la semaine du {date}"),
  ("unknown mode “{mode}” — use `day` or `week`", "mode « {mode} » inconnu — utilise `day` ou `week`"),
  ("invalid settings: {error}", "réglages invalides : {error}"),
  ("give `month`, or `from` and `to`", "précise `month`, ou `from` et `to`"),
  // -- timer ----------------------------------------------------------------
  ("a timer has been running since {since} — stop it first", "un chronomètre tourne déjà depuis {since} — arrête-le d'abord"),
  ("no timer running", "aucun chronomètre en cours"),
  // -- catalogue ------------------------------------------------------------
  ("`{field}` missing", "`{field}` manquant"),
  ("{count} entries still use “{id}” — archive it, or retry with ?force=true", "{count} saisie(s) utilisent encore « {id} » — archive-le, ou relance avec ?force=true"),
  ("SageX number {number} is already used by “{name}”", "le numéro SageX {number} est déjà utilisé par « {name} »"),
  ("no item “{id}” in {kind}", "aucun élément « {id} » dans {kind}"),
  ("unknown list “{kind}” — use projects, activities or presets", "liste « {kind} » inconnue — utilise projects, activities ou presets"),
  // -- month report and export ---------------------------------------------
  ("unknown aggregation “{value}”", "agrégation « {value} » inconnue"),
  ("unknown comment style “{value}”", "style de commentaire « {value} » inconnu"),
  ("{count} error(s) block the export: {errors} — fix them, or retry with ?force=true", "{count} erreur(s) bloquent l'export : {errors} — corrige-les, ou relance avec ?force=true"),
  ("no line to export for {month}", "aucune ligne à exporter pour {month}"),
  ("employee number or person number missing in the settings: the SageX import would fail", "n° de collaborateur ou n° de personne manquant dans les réglages : l'import SageX échouerait"),
  ("project “{id}” is not in the catalogue", "projet « {id} » absent du catalogue"),
  ("activity “{id}” is not in the catalogue", "activité « {id} » absente du catalogue"),
  ("project “{name}” is excluded from the export", "projet « {name} » exclu de l'export"),
  ("{date} ({hours}) — {reason}", "{date} ({hours}) — {reason}"),
  ("the comment of project {project} / activity {activity} reaches the {max}-character limit and will be cut — switch the comment style to “compact” or split the line", "le commentaire du projet {project} / activité {activity} atteint la limite de {max} caractères et sera tronqué — passe le style de commentaire en « compact » ou découpe la ligne"),
  ("entry dated {date} while the exported month is {month}", "saisie datée du {date} alors que le mois exporté est {month}"),
  ("entry without duration on {date}", "saisie sans durée le {date}"),
  ("{hours} entered on {date}", "{hours} imputées le {date}"),
  ("on {date}, “{first}” ends at {end} but “{second}” starts at {start}", "le {date}, « {first} » finit à {end} mais « {second} » commence à {start}"),
  ("{total} entered for {expected} expected ({days} working days) — gap of {gap}", "{total} saisies pour {expected} attendues ({days} jours ouvrables) — écart de {gap}"),
  ("{id} (unknown)", "{id} (inconnu)"),
  ("(no comment)", "(sans commentaire)"),
  ("{label} ({count}×: {days})", "{label} ({count}× : {days})"),
  // -- holidays (Valais) ----------------------------------------------------
  ("New Year's Day", "Nouvel An"),
  ("Saint Joseph's Day", "Saint-Joseph"),
  ("Swiss National Day", "Fête nationale"),
  ("Assumption", "Assomption"),
  ("All Saints' Day", "Toussaint"),
  ("Immaculate Conception", "Immaculée Conception"),
  ("Christmas", "Noël"),
  ("Easter Monday", "Lundi de Pâques"),
  ("Ascension", "Ascension"),
  ("Whit Monday", "Lundi de Pentecôte"),
  ("Corpus Christi", "Fête-Dieu"),
  ("Holiday (settings)", "Férié (réglages)"),
  // -- backup and restore ---------------------------------------------------
  ("empty archive — send the zip as the request body", "archive vide — envoie le zip dans le corps de la requête"),
  ("unreadable archive: {error}", "archive illisible : {error}"),
  ("{name} is not valid JSON: {error}", "{name} n'est pas un JSON valide : {error}"),
  ("the archive holds no file of the base", "l'archive ne contient aucun fichier de la base"),
  // -- users ----------------------------------------------------------------
  ("this instance already has several users", "cette instance a déjà plusieurs utilisateurs"),
  ("this instance has a single user — enable several users first", "cette instance n'a qu'un utilisateur — active d'abord plusieurs utilisateurs"),
  ("the identifier “{id}” is already taken", "l'identifiant « {id} » est déjà pris"),
  ("no user “{id}”", "aucun utilisateur « {id} »"),
  ("you cannot remove yourself", "tu ne peux pas te retirer toi-même"),
  ("a name is needed", "il faut un nom"),
  ("Toggl import: {error}", "import Toggl : {error}"),
];

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::BTreeSet;
  use std::path::Path;

  #[test]
  fn fill_replaces_named_placeholders_once() {
    assert_eq!(fill("entry {n}: {error}", &[("n", &3), ("error", &"zero duration")]), "entry 3: zero duration");
    // A value with braces is not expanded again; an unknown placeholder stays.
    assert_eq!(fill("{a} {b}", &[("a", &"{b}")]), "{b} {b}");
    assert_eq!(fill("no placeholder", &[]), "no placeholder");
  }

  #[test]
  fn accept_language_picks_the_preferred_supported_language() {
    assert_eq!(Lang::from_accept_language("fr-CH,fr;q=0.9,en;q=0.8"), Some(Lang::Fr));
    assert_eq!(Lang::from_accept_language("en-US,en;q=0.9,fr;q=0.8"), Some(Lang::En));
    assert_eq!(Lang::from_accept_language("de-CH,de;q=0.9,fr;q=0.8,en;q=0.7"), Some(Lang::Fr));
    assert_eq!(Lang::from_accept_language("en;q=0.5, fr"), Some(Lang::Fr));
    assert_eq!(Lang::from_accept_language("de"), None);
    assert_eq!(Lang::from_code("FR_ch"), Some(Lang::Fr));
  }

  #[tokio::test]
  async fn the_scope_sets_the_language() {
    assert_eq!(tr!("zero duration"), "zero duration");
    let french = scope(Lang::Fr, async { tr!("entry {n}: {error}", n = 2, error = tr!("zero duration")) }).await;
    assert_eq!(french, "saisie 2 : durée nulle");
  }

  /// Every `tr!("…")` of the sources has a French translation with the same
  /// placeholders, and the table holds nothing unused.
  #[test]
  fn every_message_has_a_french_translation() {
    let mut keys = BTreeSet::new();
    collect_keys(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut keys);
    assert!(keys.len() > 50, "only {} keys found — is the scan broken?", keys.len());

    let table: HashMap<&str, &str> = FR.iter().copied().collect();
    assert_eq!(table.len(), FR.len(), "a key appears twice in FR");
    let mut problems = Vec::new();
    for key in &keys {
      match table.get(key.as_str()) {
        None => problems.push(format!("missing: {key:?}")),
        Some(french) if placeholders(key) != placeholders(french) => problems.push(format!("placeholders differ: {key:?} → {french:?}")),
        _ => {}
      }
    }
    for (key, _) in FR {
      if !keys.contains(*key) {
        problems.push(format!("unused: {key:?}"));
      }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
  }

  fn placeholders(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
      let after = &rest[open + 1..];
      match after.find('}') {
        Some(close) if after[..close].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && close > 0 => {
          out.insert(after[..close].to_string());
          rest = &after[close + 1..];
        }
        _ => rest = after,
      }
    }
    out
  }

  fn collect_keys(dir: &Path, keys: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
      let path = entry.unwrap().path();
      if path.is_dir() {
        collect_keys(&path, keys);
      } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("i18n.rs") {
        let source = std::fs::read_to_string(&path).unwrap();
        let mut rest = source.as_str();
        while let Some(at) = rest.find("tr!(") {
          // Not the end of another macro's name (`include_str!(`).
          let glued = rest[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_');
          rest = &rest[at + 4..];
          if glued {
            continue;
          }
          let trimmed = rest.trim_start();
          if let Some(literal) = trimmed.strip_prefix('"') {
            let mut key = String::new();
            let mut chars = literal.chars();
            while let Some(c) = chars.next() {
              match c {
                '\\' => key.push(chars.next().unwrap_or('\\')),
                '"' => break,
                c => key.push(c),
              }
            }
            keys.insert(key);
          }
        }
      }
    }
  }
}
