//! Addresses that must never be published.
//!
//! The rule has two moments. Before a probe, the entry address decides whether a
//! node is worth running at all. After a probe, the measured exit address is the
//! one a user would actually be seen from, so a node that enters somewhere clean
//! and leaves through a blacklisted address is still dead.

pub mod list;
pub mod resolve;

pub use list::{Blacklist, Entry};
pub use resolve::Resolved;

use std::net::IpAddr;

use crate::models::Candidate;

/// The result of the entry-side check.
#[derive(Clone, Debug)]
pub struct Hit {
    pub address: IpAddr,
    pub entry: String,
}

/// Check a candidate's entry addresses against the list.
///
/// Both the written address (when it is a literal) and every DNS answer for a
/// hostname are tested: a node whose name resolves to any blacklisted address is
/// that address for the purpose of the list.
pub fn entry_hit(candidate: &Candidate, list: &Blacklist, resolved: &Resolved) -> Option<Hit> {
    if list.is_empty() {
        return None;
    }
    let addresses = resolved.get(&candidate.node.address);
    list.hit_any(addresses).map(|(address, entry)| Hit {
        address,
        entry: entry.raw.clone(),
    })
}

/// Check a measured exit address.
pub fn exit_hit(exit: Option<IpAddr>, list: &Blacklist) -> Option<Hit> {
    let exit = exit?;
    list.hit(exit).map(|entry| Hit {
        address: exit,
        entry: entry.raw.clone(),
    })
}
