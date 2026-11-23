//! Merge fetched sources into one candidate list, with provenance.
//!
//! Three things happen in a fixed order, and the order matters:
//!
//! 1. each source is capped, so one export cannot fill the whole run,
//! 2. sources are interleaved, so a global cap is not a source filter,
//! 3. duplicates are removed by the strict identity, and the surviving node is
//!    credited to the source that introduced it.

use std::collections::HashMap;

use crate::dedup;
use crate::models::{Candidate, SourceStat};

use crate::sources::fetch::FetchedSource;

#[derive(Clone, Debug, Default)]
pub struct Merged {
    pub candidates: Vec<Candidate>,
    pub stats: Vec<SourceStat>,
    /// Link-shaped lines across all bodies, before parsing.
    pub total_cfg: usize,
    /// Nodes the parser understood.
    pub total_parsed_cfg: usize,
    pub duplicates: usize,
    /// Candidates removed by `--max-nodes`.
    pub truncated: usize,
}

/// Build the run's candidate list.
///
/// `per_source` is a soft cap applied before deduplication; zero means no cap.
/// `max_nodes` is the hard cap applied after, and the number it removes is
/// reported rather than hidden.
pub fn merge(fetched: &[FetchedSource], per_source: usize, max_nodes: Option<usize>) -> Merged {
    let mut stats: Vec<SourceStat> = Vec::with_capacity(fetched.len());
    let mut groups: Vec<Vec<Candidate>> = Vec::with_capacity(fetched.len());
    let mut total_cfg = 0;
    let mut total_parsed_cfg = 0;

    for source in fetched {
        let parsed = source.nodes.len();
        let kept = if per_source == 0 {
            parsed
        } else {
            parsed.min(per_source)
        };
        total_cfg += source.raw_links;
        total_parsed_cfg += parsed;

        stats.push(SourceStat {
            name: source.entry.name.clone(),
            url: source.entry.url.clone(),
            raw_links: source.raw_links,
            parsed,
            unique: 0,
            alive: 0,
            passed: 0,
            error: source.error.clone(),
            fetch_ms: source.took_ms,
        });

        groups.push(
            source
                .nodes
                .iter()
                .take(kept)
                .cloned()
                .map(|node| Candidate {
                    node,
                    source: source.entry.name.clone(),
                })
                .collect(),
        );
    }

    let interleaved = dedup::interleave(groups);
    let (mut candidates, duplicates) = dedup::unique_counted(interleaved);

    let mut truncated = 0;
    if let Some(limit) = max_nodes {
        if candidates.len() > limit {
            truncated = candidates.len() - limit;
            candidates.truncate(limit);
        }
    }

    // `unique` is the number a source is credited with *in this run*: after the
    // global cap. Counting it earlier would make the report describe a run that
    // did not happen.
    let mut credited: HashMap<&str, usize> = HashMap::new();
    for candidate in &candidates {
        *credited.entry(candidate.source.as_str()).or_default() += 1;
    }
    for stat in &mut stats {
        stat.unique = credited.get(stat.name.as_str()).copied().unwrap_or(0);
    }

    Merged {
        candidates,
        stats,
        total_cfg,
        total_parsed_cfg,
        duplicates,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::entry::SourceEntry;
    use v2keylib::node::{Node, Protocol};

    fn fetched(name: &str, ports: &[u16]) -> FetchedSource {
        FetchedSource {
            entry: SourceEntry {
                name: name.into(),
                url: format!("https://example.com/{name}"),
            },
            nodes: ports
                .iter()
                .map(|port| {
                    Node::new(Protocol::Vless, "192.0.2.1", *port)
                        .with_uuid("00000000-0000-0000-0000-000000000001")
                })
                .collect(),
            raw_links: ports.len(),
            error: None,
            took_ms: 1,
        }
    }

    #[test]
    fn cap_is_shared_between_sources() {
        let merged = merge(
            &[fetched("a", &[1, 2, 3]), fetched("b", &[4, 5, 6])],
            0,
            Some(4),
        );
        assert_eq!(merged.candidates.len(), 4);
        assert_eq!(merged.truncated, 2);
        assert_eq!(merged.stats[0].unique, 2);
        assert_eq!(merged.stats[1].unique, 2);
    }

    #[test]
    fn duplicate_is_credited_once() {
        let merged = merge(&[fetched("a", &[443]), fetched("b", &[443])], 0, None);
        assert_eq!(merged.candidates.len(), 1);
        assert_eq!(merged.duplicates, 1);
        assert_eq!(merged.stats[0].unique, 1);
        assert_eq!(merged.stats[1].unique, 0);
    }
}
