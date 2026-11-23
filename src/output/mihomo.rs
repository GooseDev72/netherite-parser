//! `sub/mihomo`: the same nodes as a Clash.Meta document.
//!
//! It is rendered by the library rather than assembled here, because the field
//! names differ per protocol (`servername` vs `sni`, `alterId`, `reality-opts`)
//! and a hand-written map is how a `vmess` node ends up missing the two fields
//! mihomo requires. The group is a plain `select` over every node, matching the
//! reference format in `clashmeta.txt`.

use v2keylib::config::{self, Format};
use v2keylib::node::Node;

use crate::error::Result;

/// A valid document with no proxies.
///
/// mihomo refuses an empty `proxies` list in some builds, but a config with a
/// single `DIRECT` group is still a valid import and says "nothing passed"
/// instead of producing a file the client cannot open.
const EMPTY: &str = "mixed-port: 7890\nallow-lan: false\nmode: rule\nlog-level: warning\nipv6: false\nproxies: []\nproxy-groups:\n  - name: PROXY\n    type: select\n    proxies:\n      - DIRECT\nrules:\n  - MATCH,PROXY\n";

pub fn render(nodes: &[Node]) -> Result<String> {
    if nodes.is_empty() {
        return Ok(EMPTY.to_string());
    }
    Ok(config::render(Format::MihomoYaml, nodes, None)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_a_usable_document() {
        let text = render(&[]).unwrap();
        assert!(text.contains("proxies: []"));
        assert!(text.contains("MATCH,PROXY"));
    }
}
