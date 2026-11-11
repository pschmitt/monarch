//! Resolving a user-supplied name or id to one of a list of candidates.

use std::fmt;

/// Something that can be addressed by id or by one of several names.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: u64,
    pub names: Vec<String>,
}

impl Candidate {
    pub fn new(id: u64, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            id,
            names: names
                .into_iter()
                .map(Into::into)
                .filter(|n: &String| !n.is_empty())
                .collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResolveError {
    NotFound {
        query: String,
        kind: &'static str,
    },
    Ambiguous {
        query: String,
        kind: &'static str,
        matches: Vec<String>,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { query, kind } => write!(f, "no {kind} matches {query:?}"),
            Self::Ambiguous {
                query,
                kind,
                matches,
            } => write!(
                f,
                "{query:?} is ambiguous, it matches {} {kind}s: {}",
                matches.len(),
                matches.join(", ")
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Finds the index of the candidate `query` refers to.
///
/// In order: an exact id, an exact name (case-insensitive), the short name
/// (up to the first dot) and finally a unique name prefix. The first tier that
/// matches decides; several matches in one tier are an error that lists them.
pub fn resolve(
    candidates: &[Candidate],
    query: &str,
    kind: &'static str,
) -> Result<usize, ResolveError> {
    let query = query.trim();
    let needle = query.to_lowercase();

    if let Ok(id) = query.parse::<u64>()
        && let Some(index) = candidates.iter().position(|c| c.id == id)
    {
        return Ok(index);
    }

    let tiers: [&dyn Fn(&str) -> bool; 3] = [
        &|name| name == needle,
        &|name| name.split('.').next() == Some(needle.as_str()),
        &|name| name.starts_with(&needle),
    ];
    for matches in tiers {
        let hits: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.names.iter().any(|n| matches(&n.to_lowercase())))
            .map(|(i, _)| i)
            .collect();
        match hits.as_slice() {
            [] => {}
            [one] => return Ok(*one),
            many => {
                return Err(ResolveError::Ambiguous {
                    query: query.to_owned(),
                    kind,
                    matches: many.iter().map(|&i| describe(&candidates[i])).collect(),
                });
            }
        }
    }
    Err(ResolveError::NotFound {
        query: query.to_owned(),
        kind,
    })
}

fn describe(candidate: &Candidate) -> String {
    match candidate.names.first() {
        Some(name) => format!("{name} (#{})", candidate.id),
        None => format!("#{}", candidate.id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Vec<Candidate> {
        vec![
            Candidate::new(1, ["Web One", "web-1.example.com"]),
            Candidate::new(2, ["db-1.example.com"]),
            Candidate::new(3, ["db-2.example.com", "Database Two"]),
            Candidate::new(12, ["12"]),
        ]
    }

    #[test]
    fn by_id_name_and_display_name() {
        let h = hosts();
        assert_eq!(resolve(&h, "2", "host"), Ok(1));
        assert_eq!(resolve(&h, "web-1.example.com", "host"), Ok(0));
        assert_eq!(resolve(&h, "WEB ONE", "host"), Ok(0));
        assert_eq!(resolve(&h, "database two", "host"), Ok(2));
    }

    #[test]
    fn id_wins_over_a_numeric_name() {
        let h = vec![Candidate::new(12, ["x"]), Candidate::new(1, ["12"])];
        assert_eq!(resolve(&h, "12", "host"), Ok(0));
    }

    #[test]
    fn short_name_and_unique_prefix() {
        let h = hosts();
        assert_eq!(resolve(&h, "db-2", "host"), Ok(2));
        assert_eq!(resolve(&h, "web", "host"), Ok(0));
    }

    #[test]
    fn ambiguity_lists_the_candidates() {
        let err = resolve(&hosts(), "db", "host").unwrap_err();
        let ResolveError::Ambiguous { matches, .. } = &err else {
            panic!("expected ambiguity, got {err:?}");
        };
        assert_eq!(matches.len(), 2);
        let text = err.to_string();
        assert!(text.contains("db-1.example.com (#2)"), "{text}");
        assert!(text.contains("db-2.example.com (#3)"), "{text}");
    }

    #[test]
    fn exact_name_beats_prefix_ambiguity() {
        let h = vec![Candidate::new(1, ["db"]), Candidate::new(2, ["db-backup"])];
        assert_eq!(resolve(&h, "db", "host"), Ok(0));
    }

    #[test]
    fn unknown_name() {
        assert_eq!(
            resolve(&hosts(), "nope", "host"),
            Err(ResolveError::NotFound {
                query: "nope".into(),
                kind: "host"
            })
        );
    }
}
