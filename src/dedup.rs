//! Stub: deduplication moved to netherite-core pipeline evolution.

use std::collections::HashSet;
use crate::models::Candidate;

pub fn unique_counted(candidates: Vec<Candidate>) -> (Vec<Candidate>, usize) {
    let mut seen: HashSet<String> = HashSet::with_capacity(candidates.len());
    let mut out = Vec::with_capacity(candidates.len());
    let mut dropped = 0;
    for c in candidates {
        if seen.insert(c.node.strict_key()) { out.push(c); } else { dropped += 1; }
    }
    (out, dropped)
}

pub fn interleave(groups: Vec<Vec<Candidate>>) -> Vec<Candidate> {
    let longest = groups.iter().map(Vec::len).max().unwrap_or(0);
    let mut iters: Vec<_> = groups.into_iter().map(|g| g.into_iter()).collect();
    let mut out = Vec::new();
    for _ in 0..longest {
        for it in &mut iters { if let Some(c) = it.next() { out.push(c); } }
    }
    out
}
