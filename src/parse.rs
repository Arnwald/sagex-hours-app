//! Entrées « tolérantes » : durées écrites à la main et références au catalogue
//! désignées par identifiant, numéro SageX ou nom.
//!
//! C'est ce qui permet d'écrire `{"project": "Subice", "duration": "4h30"}`
//! plutôt que de connaître les identifiants par cœur.

use crate::model::{Activity, Catalog, Preset, Project};
use serde::Deserialize;

/// Une valeur JSON qui peut être un nombre ou une chaîne.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Loose {
  Number(f64),
  Text(String),
}

impl Loose {
  pub fn as_text(&self) -> String {
    match self {
      Loose::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
      Loose::Number(n) => format!("{n}"),
      Loose::Text(s) => s.trim().to_string(),
    }
  }
}

/// `"1h30"`, `"1:30"`, `"90"`, `"90m"`, `"1.5h"`, `"1,5h"` → 90 minutes.
///
/// Un nombre nu est interprété en minutes ; utiliser `hours` pour des heures.
pub fn parse_minutes(input: &str) -> Result<u32, String> {
  let raw = input.trim().to_lowercase().replace(',', ".");
  if raw.is_empty() {
    return Err("durée vide".to_string());
  }

  // Chaque règle est essayée dans l'ordre ; une règle qui ne s'applique pas
  // laisse la main à la suivante plutôt que d'échouer (« 1h30min » doit passer).
  if let Some(hours) = suffixed(&raw, &["heures", "heure", "hours", "hour"]) {
    return finish(hours * 60.0, input);
  }
  if let Some(minutes) = suffixed(&raw, &["minutes", "minute", "mins", "min", "m"]) {
    return finish(minutes, input);
  }
  if let Some((h, m)) = raw.split_once('h') {
    let hours = parse_or_zero(h);
    let minutes = parse_or_zero(m.trim().trim_end_matches("min").trim_end_matches('m'));
    if let (Some(hours), Some(minutes)) = (hours, minutes) {
      return finish(hours * 60.0 + minutes, input);
    }
  }
  if let Some((h, m)) = raw.split_once(':') {
    if let (Some(hours), Some(minutes)) = (parse_or_zero(h), parse_or_zero(m)) {
      return finish(hours * 60.0 + minutes, input);
    }
  }
  match raw.parse::<f64>() {
    Ok(minutes) => finish(minutes, input),
    Err(_) => Err(bad(input)),
  }
}

/// Valeur numérique précédant l'un des suffixes, si elle est exploitable.
fn suffixed(raw: &str, suffixes: &[&str]) -> Option<f64> {
  for suffix in suffixes {
    if let Some(value) = raw.strip_suffix(suffix) {
      if let Ok(number) = value.trim().parse::<f64>() {
        return Some(number);
      }
    }
  }
  None
}

/// `""` vaut 0 ; toute autre valeur doit être un nombre.
fn parse_or_zero(part: &str) -> Option<f64> {
  let part = part.trim();
  if part.is_empty() {
    Some(0.0)
  } else {
    part.parse().ok()
  }
}

fn finish(minutes: f64, input: &str) -> Result<u32, String> {
  if !minutes.is_finite() || minutes < 0.0 {
    return Err(bad(input));
  }
  if minutes > 24.0 * 60.0 {
    return Err(format!("durée « {input} » supérieure à 24 h"));
  }
  Ok(minutes.round() as u32)
}

fn bad(input: &str) -> String {
  format!("durée « {input} » incomprise — exemples : 1h30, 1:30, 90, 90min, 1.5h")
}

/// Résultat d'une recherche dans une des listes du catalogue.
pub enum Lookup<T> {
  Found(T),
  NotFound(String),
  Ambiguous(String),
}

impl<T> Lookup<T> {
  pub fn into_result(self) -> Result<T, String> {
    match self {
      Lookup::Found(v) => Ok(v),
      Lookup::NotFound(msg) | Lookup::Ambiguous(msg) => Err(msg),
    }
  }
}

/// Recherche générique : identifiant exact, puis numéro SageX, puis nom exact,
/// puis nom insensible à la casse, puis préfixe unique.
fn lookup<'a, T>(
  items: &'a [T],
  needle: &str,
  kind: &str,
  id_of: impl Fn(&T) -> &str,
  number_of: impl Fn(&T) -> Option<u32>,
  name_of: impl Fn(&T) -> &str,
) -> Lookup<&'a T> {
  let needle = needle.trim();
  if needle.is_empty() {
    return Lookup::NotFound(format!("{kind} non précisé"));
  }
  if let Some(found) = items.iter().find(|i| id_of(i) == needle) {
    return Lookup::Found(found);
  }
  if let Ok(number) = needle.parse::<u32>() {
    if let Some(found) = items.iter().find(|i| number_of(i) == Some(number)) {
      return Lookup::Found(found);
    }
  }
  if let Some(found) = items.iter().find(|i| name_of(i) == needle) {
    return Lookup::Found(found);
  }

  let lower = needle.to_lowercase();
  let insensitive: Vec<&T> = items.iter().filter(|i| name_of(i).to_lowercase() == lower).collect();
  if insensitive.len() == 1 {
    return Lookup::Found(insensitive[0]);
  }

  let partial: Vec<&T> = items
    .iter()
    .filter(|i| name_of(i).to_lowercase().contains(&lower) || id_of(i).contains(&lower))
    .collect();
  match partial.len() {
    1 => Lookup::Found(partial[0]),
    0 => Lookup::NotFound(format!("{kind} « {needle} » introuvable")),
    _ => Lookup::Ambiguous(format!(
      "{kind} « {needle} » ambigu — candidats : {}",
      partial.iter().map(|i| id_of(i)).collect::<Vec<_>>().join(", ")
    )),
  }
}

pub fn find_project<'a>(catalog: &'a Catalog, needle: &str) -> Lookup<&'a Project> {
  lookup(&catalog.projects, needle, "projet", |p| &p.id, |p| Some(p.sagex_number), |p| &p.name)
}

pub fn find_activity<'a>(catalog: &'a Catalog, needle: &str) -> Lookup<&'a Activity> {
  lookup(&catalog.activities, needle, "activité", |a| &a.id, |a| Some(a.sagex_number), |a| &a.name)
}

pub fn find_preset<'a>(catalog: &'a Catalog, needle: &str) -> Lookup<&'a Preset> {
  lookup(&catalog.presets, needle, "tâche", |p| &p.id, |_| None, |p| &p.label)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Activity, Preset, Project};

  #[test]
  fn durations_accept_the_usual_spellings() {
    for (input, expected) in [
      ("1h30", 90),
      ("1H30", 90),
      ("1h", 60),
      ("2h05", 125),
      ("1:30", 90),
      ("90", 90),
      ("90min", 90),
      ("90 m", 90),
      ("1.5h", 90),
      ("1,5h", 90),
      ("2 heures", 120),
      ("0h15", 15),
      ("1h30min", 90),
      ("45 minutes", 45),
      ("h30", 30),
    ] {
      assert_eq!(parse_minutes(input), Ok(expected), "pour « {input} »");
    }
  }

  #[test]
  fn nonsense_durations_are_rejected_with_help() {
    assert!(parse_minutes("").is_err());
    assert!(parse_minutes("hier").unwrap_err().contains("exemples"));
    assert!(parse_minutes("-3h").is_err());
    assert!(parse_minutes("30h").unwrap_err().contains("24"));
  }

  fn catalog() -> Catalog {
    Catalog {
      projects: vec![
        Project {
          id: "spl-base".into(),
          sagex_number: 89403,
          name: "SPL Base".into(),
          color: "#000".into(),
          archived: false,
          exportable: true,
          note: None,
        },
        Project {
          id: "subice-marvis-fns-sefri".into(),
          sagex_number: 129030,
          name: "SUBICE (MARVIS) FNS/SEFRI".into(),
          color: "#000".into(),
          archived: false,
          exportable: true,
          note: None,
        },
      ],
      activities: vec![
        Activity { id: "seance".into(), sagex_number: 4, name: "Séance de travail".into(), archived: false, note: None },
        Activity { id: "ra-d".into(), sagex_number: 29, name: "Réalisation de projets ou d'activités Ra&D".into(), archived: false, note: None },
      ],
      presets: vec![Preset {
        id: "subice-ra-d-subice".into(),
        label: "Ra&D Subice".into(),
        project_id: "subice-marvis-fns-sefri".into(),
        activity_id: "ra-d".into(),
        comment: None,
        color: None,
        toggl: None,
        archived: false,
      }],
    }
  }

  #[test]
  fn references_resolve_by_id_number_name_or_fragment() {
    let c = catalog();
    assert_eq!(find_project(&c, "spl-base").into_result().unwrap().id, "spl-base");
    assert_eq!(find_project(&c, "89403").into_result().unwrap().id, "spl-base");
    assert_eq!(find_project(&c, "SPL Base").into_result().unwrap().id, "spl-base");
    assert_eq!(find_project(&c, "subice").into_result().unwrap().sagex_number, 129030);
    assert_eq!(find_activity(&c, "29").into_result().unwrap().id, "ra-d");
    assert_eq!(find_activity(&c, "séance").into_result().unwrap().id, "seance");
    assert_eq!(find_preset(&c, "Ra&D Subice").into_result().unwrap().id, "subice-ra-d-subice");
  }

  #[test]
  fn unknown_and_ambiguous_references_explain_themselves() {
    let c = catalog();
    let err = find_project(&c, "Steiger").into_result().unwrap_err();
    assert!(err.contains("introuvable"), "{err}");

    let err = find_activity(&c, "a").into_result().unwrap_err();
    assert!(err.contains("ambigu") && err.contains("seance"), "{err}");
  }
}
