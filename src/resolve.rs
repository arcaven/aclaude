//! Fuzzy resolution for --theme and --persona CLI inputs.
//!
//! Under the B14 agent taxonomy there are ~100 themes and ~1050 characters.
//! Exact-slug-only lookup is tedious; this module lets users type fragments
//! (kubectl-style partial IDs plus fzf-style subsequence match) and get the
//! intended slug back.
//!
//! Design (see aae-orc-jwqz):
//!
//! Per-identifier resolution order (`match_slug`):
//! 1. Exact slug match.
//! 2. Unique case-insensitive prefix match.
//! 3. Fuzzy subsequence match (nucleo-matcher):
//!    a. Above min-score AND top score beats runner-up by score-gap →
//!    unambiguous; use top match.
//!    b. Above min-score AND tied with runner-up → ambiguous; use top
//!    but emit stderr warning with top 5 candidates.
//!    c. No candidate above min-score → NotFound (caller errors with
//!    top candidates).
//!
//! Two-phase theme+persona resolver (`resolve_theme_and_persona`):
//! * Resolve `--theme` first. When it succeeds, narrow `--persona`
//!   matching to that theme's ~10 characters — ~100× fewer candidates
//!   drops ambiguity to near-zero.
//! * If `--theme` is missing or fails to resolve, and `--persona` is
//!   given, search characters across every theme and back-propagate the
//!   matched character's home theme.

use std::cmp::Reverse;
use std::io::Write;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::error::{ForestageError, Result};
use crate::persona::{self, ThemeFile};

/// Minimum fuzzy score we'll accept. `fuzzy-matcher` returns scores
/// on the order of tens-to-low-hundreds for reasonable matches; below
/// this we treat the result as noise.
const MIN_FUZZY_SCORE: i64 = 40;

/// Score gap between rank-1 and rank-2 that makes a fuzzy match
/// unambiguous. Below this the caller still proceeds with rank-1 but
/// emits a disambiguation warning on stderr.
const FUZZY_AMBIGUITY_GAP: i64 = 15;

/// How many candidates we include in "did you mean?" output.
const MAX_CANDIDATES_SHOWN: usize = 5;

/// Resolution outcome for a single fuzzy lookup.
#[derive(Debug)]
pub enum MatchResult<T> {
    /// Exact slug match — caller should use with no ceremony.
    Exact(T),
    /// Unique prefix match — one candidate starts with the query.
    Prefix(T),
    /// Fuzzy match, unambiguous (rank-1 beats rank-2 by the gap).
    FuzzyUnique(T),
    /// Fuzzy match, ambiguous — caller should warn with candidates but
    /// proceed with the top match.
    FuzzyAmbiguous { top: T, candidates: Vec<T> },
    /// No candidate met the minimum score. Caller should error and show
    /// the top candidates (may be empty if nothing matched at all).
    NotFound { candidates: Vec<T> },
}

impl<T: Clone> MatchResult<T> {
    /// Pick the resolved value, if any. `NotFound` yields None.
    pub fn picked(&self) -> Option<T> {
        match self {
            MatchResult::Exact(v)
            | MatchResult::Prefix(v)
            | MatchResult::FuzzyUnique(v)
            | MatchResult::FuzzyAmbiguous { top: v, .. } => Some(v.clone()),
            MatchResult::NotFound { .. } => None,
        }
    }
}

/// Match a free-form query against a slice of canonical slugs.
///
/// Empty query returns `NotFound` with no candidates; callers should
/// skip calling this when no user input exists.
pub fn match_slug(query: &str, candidates: &[String]) -> MatchResult<String> {
    if query.is_empty() || candidates.is_empty() {
        return MatchResult::NotFound {
            candidates: Vec::new(),
        };
    }

    // 1. Exact match (case-insensitive).
    for c in candidates {
        if c.eq_ignore_ascii_case(query) {
            return MatchResult::Exact(c.clone());
        }
    }

    // 2. Unique prefix match.
    let q_lower = query.to_ascii_lowercase();
    let prefix_hits: Vec<&String> = candidates
        .iter()
        .filter(|c| c.to_ascii_lowercase().starts_with(&q_lower))
        .collect();
    if prefix_hits.len() == 1 {
        return MatchResult::Prefix(prefix_hits[0].clone());
    }

    // 3. Fuzzy subsequence via fuzzy-matcher (skim's matcher — MIT).
    let matcher = SkimMatcherV2::default().ignore_case();
    let mut scored: Vec<(String, i64)> = candidates
        .iter()
        .filter_map(|c| matcher.fuzzy_match(c, query).map(|s| (c.clone(), s)))
        .collect();
    scored.sort_by_key(|(_, s)| Reverse(*s));

    // Filter to candidates above the minimum score.
    let qualifying: Vec<&(String, i64)> = scored
        .iter()
        .filter(|(_, s)| *s >= MIN_FUZZY_SCORE)
        .collect();
    if qualifying.is_empty() {
        let fallback: Vec<String> = scored
            .into_iter()
            .take(MAX_CANDIDATES_SHOWN)
            .map(|(c, _)| c)
            .collect();
        return MatchResult::NotFound {
            candidates: fallback,
        };
    }

    let top = qualifying[0].0.clone();
    let top_score = qualifying[0].1;
    let second_score = qualifying.get(1).map(|(_, s)| *s).unwrap_or(0);

    if top_score.saturating_sub(second_score) >= FUZZY_AMBIGUITY_GAP {
        return MatchResult::FuzzyUnique(top);
    }

    let candidates: Vec<String> = qualifying
        .into_iter()
        .take(MAX_CANDIDATES_SHOWN)
        .map(|(c, _)| c.clone())
        .collect();
    MatchResult::FuzzyAmbiguous { top, candidates }
}

/// Resolve a theme query against the embedded theme slugs.
pub fn match_theme(query: &str) -> MatchResult<String> {
    let themes = persona::list_themes();
    match_slug(query, &themes)
}

/// Resolve a character query within a single theme's roster.
pub fn match_character_in_theme(query: &str, theme: &ThemeFile) -> MatchResult<String> {
    let slugs: Vec<String> = theme.characters.keys().cloned().collect();
    match_slug(query, &slugs)
}

/// Global character search across every theme. Returns (theme_slug,
/// character_slug) pairs.
///
/// Useful when the user supplies `--persona` without `--theme` or when
/// `--theme` failed to resolve.
pub fn match_character_globally(query: &str) -> MatchResult<(String, String)> {
    // Build the qualified-slug list: "theme/character" — the fuzzy
    // matcher operates on these and we split back on return.
    let themes = persona::list_themes();
    let mut qualified: Vec<String> = Vec::new();
    let mut lookup: Vec<(String, String)> = Vec::new();
    for theme_slug in &themes {
        let Ok(theme) = persona::load_theme(theme_slug) else {
            continue;
        };
        for char_slug in theme.characters.keys() {
            qualified.push(format!("{theme_slug}/{char_slug}"));
            lookup.push((theme_slug.clone(), char_slug.clone()));
        }
    }
    // The actual query fuzzy-matches against just the character-slug half —
    // but we also let the theme half contribute (so "discworld/granny"
    // queries work). nucleo handles the / separator fine.
    let result = match_slug(query, &qualified);
    // Map strings back to (theme, char) pairs.
    let map = |q: String| -> (String, String) {
        let pos = qualified.iter().position(|s| s == &q).unwrap_or(0);
        lookup[pos].clone()
    };
    match result {
        MatchResult::Exact(q) => MatchResult::Exact(map(q)),
        MatchResult::Prefix(q) => MatchResult::Prefix(map(q)),
        MatchResult::FuzzyUnique(q) => MatchResult::FuzzyUnique(map(q)),
        MatchResult::FuzzyAmbiguous { top, candidates } => MatchResult::FuzzyAmbiguous {
            top: map(top),
            candidates: candidates.into_iter().map(map).collect(),
        },
        MatchResult::NotFound { candidates } => MatchResult::NotFound {
            candidates: candidates.into_iter().map(map).collect(),
        },
    }
}

/// Two-phase resolve: theme first (to narrow), persona second. Returns
/// canonical slugs. `None`/`None` means the user supplied neither; the
/// caller should fall back to config defaults.
///
/// Emits stderr warnings for ambiguous or fallback resolutions so the
/// user can see what we did (per session-032 design decision).
pub fn resolve_theme_and_persona(
    theme_q: Option<&str>,
    persona_q: Option<&str>,
) -> Result<(Option<String>, Option<String>)> {
    // Phase 1 — resolve theme.
    let theme_resolved: Option<String> = match theme_q {
        None | Some("") => None,
        Some(q) => {
            let m = match_theme(q);
            emit_warning_if_fuzzy(q, "theme", &m);
            match &m {
                MatchResult::NotFound { .. } => None,
                _ => m.picked(),
            }
        }
    };

    // Phase 2 — resolve persona.
    let persona_q_nonempty = persona_q.filter(|q| !q.is_empty());
    let persona_resolved: Option<(String, String)> = match (
        theme_resolved.as_deref(),
        persona_q_nonempty,
    ) {
        // Theme resolved, persona given — narrow to that theme.
        (Some(t), Some(pq)) => {
            let theme = persona::load_theme(t)?;
            let m = match_character_in_theme(pq, &theme);
            emit_warning_if_fuzzy(pq, "persona", &m);
            match m {
                MatchResult::NotFound { candidates } => {
                    return Err(ForestageError::CharacterNotFound {
                        character: format!(
                            "{pq} (no match; candidates: {})",
                            format_candidates(&candidates)
                        ),
                        theme: theme.theme.name,
                    });
                }
                other => other.picked().map(|p| (t.to_string(), p)),
            }
        }
        // No theme, persona given — global search + back-prop.
        (None, Some(pq)) => {
            let m = match_character_globally(pq);
            let (theme_slug, char_slug) = match m {
                MatchResult::NotFound { candidates } => {
                    return Err(ForestageError::CharacterNotFound {
                        character: format!(
                            "{pq} (no match; candidates: {})",
                            candidates
                                .iter()
                                .map(|(t, c)| format!("{c} ({t})"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        theme: "(global)".into(),
                    });
                }
                MatchResult::Exact(v) | MatchResult::Prefix(v) | MatchResult::FuzzyUnique(v) => v,
                MatchResult::FuzzyAmbiguous { top, candidates } => {
                    let rendered: Vec<String> = candidates
                        .iter()
                        .map(|(t, c)| format!("{c} ({t})"))
                        .collect();
                    warn_stderr(&format!(
                        "persona '{pq}' is ambiguous — proceeding with top match. candidates: {}",
                        rendered.join(", ")
                    ));
                    top
                }
            };
            // If the user asked for a theme but we couldn't resolve it,
            // note the back-propagation explicitly.
            if let Some(orig) = theme_q {
                if !orig.is_empty() {
                    warn_stderr(&format!(
                        "theme '{orig}' not found; resolved via persona '{pq}' → theme={theme_slug}"
                    ));
                }
            }
            Some((theme_slug, char_slug))
        }
        // Everything else: keep theme as-is, leave persona as None.
        _ => None,
    };

    match persona_resolved {
        Some((t, p)) => Ok((Some(t), Some(p))),
        None => Ok((theme_resolved, None)),
    }
}

fn emit_warning_if_fuzzy(query: &str, kind: &str, result: &MatchResult<String>) {
    match result {
        MatchResult::FuzzyUnique(v) => {
            warn_stderr(&format!("{kind} '{query}' → {v} (fuzzy match)"));
        }
        MatchResult::FuzzyAmbiguous { top, candidates } => {
            warn_stderr(&format!(
                "{kind} '{query}' is ambiguous — proceeding with {top}. candidates: {}",
                candidates.join(", ")
            ));
        }
        MatchResult::NotFound { candidates } => {
            warn_stderr(&format!(
                "{kind} '{query}' not found. candidates: {}",
                format_candidates(candidates)
            ));
        }
        // Exact / Prefix — no warning needed.
        _ => {}
    }
}

fn format_candidates(candidates: &[String]) -> String {
    if candidates.is_empty() {
        "(none)".to_string()
    } else {
        candidates.join(", ")
    }
}

fn warn_stderr(msg: &str) {
    let _ = writeln!(std::io::stderr(), "forestage: {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> Vec<String> {
        vec![
            "discworld".into(),
            "dune".into(),
            "the-expanse".into(),
            "breaking-bad".into(),
            "alice-in-wonderland".into(),
        ]
    }

    #[test]
    fn exact_match_wins() {
        let r = match_slug("discworld", &themes());
        assert!(matches!(r, MatchResult::Exact(ref s) if s == "discworld"));
    }

    #[test]
    fn exact_match_case_insensitive() {
        let r = match_slug("Dune", &themes());
        assert!(matches!(r, MatchResult::Exact(ref s) if s == "dune"));
    }

    #[test]
    fn unique_prefix_match() {
        // "disc" only matches discworld.
        let r = match_slug("disc", &themes());
        assert!(matches!(r, MatchResult::Prefix(ref s) if s == "discworld"));
    }

    #[test]
    fn empty_query_returns_not_found() {
        let r = match_slug("", &themes());
        assert!(matches!(r, MatchResult::NotFound { .. }));
    }

    #[test]
    fn fuzzy_subsequence_matches() {
        // A longer subsequence of "discworld" — scores above MIN_FUZZY_SCORE.
        // (Very short queries like "dw" legitimately don't score high
        // enough against a 5-candidate pool; that's the min-score cut
        // doing its job.)
        let r = match_slug("dcwrld", &themes());
        assert_eq!(r.picked().as_deref(), Some("discworld"), "got {r:?}");
    }

    #[test]
    fn garbage_query_returns_not_found() {
        let r = match_slug("zzzzzz_nonexistent_xxqxx", &themes());
        assert!(matches!(r, MatchResult::NotFound { .. }));
    }

    #[test]
    fn fuzzy_abbreviation_resolves_character_in_theme() {
        // "grny" → granny-weatherwax within the discworld roster.
        let theme = persona::load_theme("discworld").expect("discworld embedded");
        let r = match_character_in_theme("grny", &theme);
        assert_eq!(
            r.picked().as_deref(),
            Some("granny-weatherwax"),
            "got {r:?}"
        );
    }

    #[test]
    fn fuzzy_initials_resolve_in_theme() {
        // "lhv" → lord-havelock-vetinari (initials of every word).
        let theme = persona::load_theme("discworld").expect("discworld embedded");
        let r = match_character_in_theme("lhv", &theme);
        assert_eq!(
            r.picked().as_deref(),
            Some("lord-havelock-vetinari"),
            "got {r:?}"
        );
    }

    #[test]
    fn match_theme_on_embedded_data() {
        // "dune" is embedded and should match exactly.
        let r = match_theme("dune");
        assert!(matches!(r, MatchResult::Exact(ref s) if s == "dune"));
    }

    #[test]
    fn match_theme_prefix_on_embedded_data() {
        // "disc" should be a unique prefix.
        let r = match_theme("disc");
        assert!(matches!(r, MatchResult::Prefix(ref s) if s == "discworld"));
    }

    #[test]
    fn global_persona_search_finds_granny() {
        let r = match_character_globally("granny-weatherwax");
        let got = r.picked().expect("granny-weatherwax should resolve");
        assert_eq!(got.0, "discworld");
        assert_eq!(got.1, "granny-weatherwax");
    }

    #[test]
    fn resolve_theme_plus_persona_narrows_correctly() {
        let (t, p) = resolve_theme_and_persona(Some("discworld"), Some("granny-weatherwax"))
            .expect("should resolve");
        assert_eq!(t.as_deref(), Some("discworld"));
        assert_eq!(p.as_deref(), Some("granny-weatherwax"));
    }

    #[test]
    fn resolve_persona_only_back_propagates_theme() {
        let (t, p) =
            resolve_theme_and_persona(None, Some("granny-weatherwax")).expect("should resolve");
        assert_eq!(t.as_deref(), Some("discworld"));
        assert_eq!(p.as_deref(), Some("granny-weatherwax"));
    }

    #[test]
    fn resolve_neither_returns_none_none() {
        let (t, p) = resolve_theme_and_persona(None, None).expect("no-op should succeed");
        assert_eq!(t, None);
        assert_eq!(p, None);
    }

    #[test]
    fn resolve_theme_only_leaves_persona_none() {
        let (t, p) = resolve_theme_and_persona(Some("dune"), None).expect("should resolve");
        assert_eq!(t.as_deref(), Some("dune"));
        assert_eq!(p, None);
    }
}
