//! `dead.txt`: the links not published.
//!
//! Plain links, one per line, so the file can be fed back as an input or used to
//! build the next blacklist. No header and no comments: this is a cache, not a
//! subscription.

use std::collections::HashSet;

use v2keylib::links;

use crate::models::Checked;

/// Render the rejected links, deduplicated.
pub fn render(checked: &[Checked]) -> String {
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for node in checked.iter().filter(|checked| !checked.is_published()) {
        if !seen.insert(node.node.strict_key()) {
            continue;
        }
        if let Ok(link) = links::render(&node.node) {
            lines.push(link);
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    lines.join("\n") + "\n"
}

/// Load the links already known to be dead, as strict identities.
///
/// The file stores links, but a link carries a name and the client may have
/// renamed it since. Parsing back to a node and taking [`Node::strict_key`] makes
/// the comparison about the endpoint and its parameters, which is what "dead"
/// means.
pub fn load_known(path: &std::path::Path) -> HashSet<String> {
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .filter_map(|line| links::parse(line).ok())
                .map(|node| node.strict_key())
                .collect()
        })
        .unwrap_or_default()
}
