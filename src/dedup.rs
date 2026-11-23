//! Deduplication and ordering for a list assembled out of many sources.
//!
//! The loose `dedup_key` is wrong here: two rows with the same address and uuid
//! but a different transport are different configs and behave differently in a
//! client. `strict_key` is the identity this builder publishes under.

use std::collections::HashSet;

use crate::models::Candidate;

/// Remove repeats and report how many were dropped.
pub fn unique_counted(candidates: Vec<Candidate>) -> (Vec<Candidate>, usize) {
    let mut seen: HashSet<String> = HashSet::with_capacity(candidates.len());
    let mut out = Vec::with_capacity(candidates.len());
    let mut dropped = 0;
    for candidate in candidates {
        if seen.insert(candidate.node.strict_key()) {
            out.push(candidate);
        } else {
            dropped += 1;
        }
    }
    (out, dropped)
}

/// Interleave groups so a cap does not become a source filter.
///
/// Sources are sorted by name and then taken one at a time. The alternative —
/// concatenation — means `--max-nodes 40` measures the alphabetically first
/// source forty times, and the report then says the other twenty sources gave
/// nothing.
pub fn interleave(groups: Vec<Vec<Candidate>>) -> Vec<Candidate> {
    let longest = groups.iter().map(Vec::len).max().unwrap_or(0);
    let mut iterators: Vec<_> = groups.into_iter().map(|group| group.into_iter()).collect();
    let mut out = Vec::new();
    for _ in 0..longest {
        for iterator in &mut iterators {
            if let Some(candidate) = iterator.next() {
                out.push(candidate);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use v2keylib::node::{Node, Protocol};

    fn candidate(address: &str, port: u16, source: &str) -> Candidate {
        Candidate {
            node: Node::new(Protocol::Vless, address, port)
                .with_uuid("00000000-0000-0000-0000-000000000001"),
            source: source.into(),
        }
    }

    #[test]
    fn repeats_are_dropped_and_counted() {
        let list = vec![
            candidate("a", 443, "one"),
            candidate("a", 443, "two"),
            candidate("b", 443, "one"),
        ];
        let (unique_list, dropped) = unique_counted(list);
        assert_eq!(unique_list.len(), 2);
        assert_eq!(dropped, 1);
        assert_eq!(unique_list[0].source, "one");
    }

    #[test]
    fn interleave_takes_from_each_source_in_turn() {
        let groups = vec![
            vec![candidate("a1", 1, "a"), candidate("a2", 2, "a")],
            vec![candidate("b1", 1, "b")],
        ];
        let order: Vec<String> = interleave(groups)
            .into_iter()
            .map(|c| c.node.address)
            .collect();
        assert_eq!(order, vec!["a1", "b1", "a2"]);
    }
}
