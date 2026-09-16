//! Fabrication d'identifiants lisibles pour le catalogue.
//!
//! Les identifiants apparaissent dans le JSON et dans les requêtes que Claude
//! écrit : ils doivent rester courts, sans accent, et reconnaissables.

/// Mots vides retirés uniquement si le slug dépasse la longueur cible.
/// Aucun mot d'une lettre ici : « Ra&D » doit rester « ra-d ».
const STOPWORDS: &[&str] = &["de", "des", "du", "la", "le", "les", "un", "une", "ou", "et", "au", "aux", "en", "pour", "sur"];

const MAX_LEN: usize = 36;

/// `"Réalisation de projets ou d'activités Ra&D"` → `"realisation-projets-activites-ra-d"`.
pub fn slugify(input: &str) -> String {
  let cleaned = strip_elisions(&deaccent(input));
  let words: Vec<String> = cleaned
    .split(|c: char| !c.is_ascii_alphanumeric())
    .filter(|w| !w.is_empty())
    .map(|w| w.to_ascii_lowercase())
    .collect();

  let full = words.join("-");
  if full.len() <= MAX_LEN {
    return fallback(full);
  }

  // Trop long : on retire les mots vides, puis on tronque sur une frontière de mot.
  let trimmed: Vec<String> = words.iter().filter(|w| !STOPWORDS.contains(&w.as_str())).cloned().collect();
  let trimmed = if trimmed.is_empty() { words } else { trimmed };

  let mut out = String::new();
  for word in trimmed {
    if out.is_empty() {
      out = word;
    } else if out.len() + 1 + word.len() <= MAX_LEN {
      out.push('-');
      out.push_str(&word);
    } else {
      break;
    }
  }
  out.truncate(MAX_LEN);
  fallback(out.trim_matches('-').to_string())
}

fn fallback(s: String) -> String {
  if s.is_empty() {
    "sans-nom".to_string()
  } else {
    s
  }
}

/// Rend `slug` unique vis-à-vis de `taken`, en suffixant si nécessaire.
pub fn unique(slug: &str, taken: &mut std::collections::HashSet<String>, hint: Option<u32>) -> String {
  if taken.insert(slug.to_string()) {
    return slug.to_string();
  }
  if let Some(hint) = hint {
    let candidate = format!("{slug}-{hint}");
    if taken.insert(candidate.clone()) {
      return candidate;
    }
  }
  for n in 2.. {
    let candidate = format!("{slug}-{n}");
    if taken.insert(candidate.clone()) {
      return candidate;
    }
  }
  unreachable!()
}

/// Supprime les élisions françaises : `d'activités` → `activités`, sans toucher
/// aux lettres isolées qui portent du sens (`Ra&D`).
fn strip_elisions(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  let mut word = String::new();
  for c in input.chars() {
    match c {
      '\'' | '\u{2019}' => {
        if word.len() <= 2 && word.chars().all(|c| c.is_ascii_alphabetic()) {
          word.clear();
        }
        out.push_str(&word);
        out.push(' ');
        word.clear();
      }
      c if c.is_ascii_alphanumeric() => word.push(c),
      _ => {
        out.push_str(&word);
        word.clear();
        out.push(' ');
      }
    }
  }
  out.push_str(&word);
  out
}

/// Translittération ASCII suffisante pour du français et de l'allemand.
fn deaccent(input: &str) -> String {
  input
    .chars()
    .map(|c| match c {
      'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
      'ç' => 'c',
      'è' | 'é' | 'ê' | 'ë' => 'e',
      'ì' | 'í' | 'î' | 'ï' => 'i',
      'ñ' => 'n',
      'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
      'ù' | 'ú' | 'û' | 'ü' => 'u',
      'ý' | 'ÿ' => 'y',
      'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
      'Ç' => 'C',
      'È' | 'É' | 'Ê' | 'Ë' => 'E',
      'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
      'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' => 'O',
      'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
      'ß' => 's',
      'œ' | 'Œ' => 'o',
      'æ' | 'Æ' => 'a',
      other => other,
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn short_names_keep_all_words() {
    assert_eq!(slugify("SPL Base"), "spl-base");
    assert_eq!(slugify("Séance de travail"), "seance-de-travail");
    assert_eq!(slugify("SUBICE (MARVIS) FNS/SEFRI"), "subice-marvis-fns-sefri");
  }

  #[test]
  fn elisions_are_removed_but_meaningful_letters_survive() {
    assert_eq!(slugify("Tâches spécifiques d'enseignement"), "taches-specifiques-enseignement");
    assert_eq!(slugify("Ra&D SPL"), "ra-d-spl");
  }

  #[test]
  fn long_names_drop_stopwords_then_truncate() {
    assert_eq!(slugify("Réalisation de projets ou d'activités Ra&D"), "realisation-projets-activites-ra-d");
    let slug = slugify("CSIA-PME (2ème année: 2023-24)");
    assert!(slug.len() <= MAX_LEN, "{slug}");
    assert!(slug.starts_with("csia-pme"), "{slug}");
  }

  #[test]
  fn degenerate_input_never_yields_empty_slug() {
    assert_eq!(slugify("///"), "sans-nom");
    assert_eq!(slugify(""), "sans-nom");
  }

  #[test]
  fn collisions_are_disambiguated() {
    let mut taken = std::collections::HashSet::new();
    assert_eq!(unique("admin", &mut taken, None), "admin");
    assert_eq!(unique("admin", &mut taken, Some(142259)), "admin-142259");
    assert_eq!(unique("admin", &mut taken, None), "admin-2");
  }
}
