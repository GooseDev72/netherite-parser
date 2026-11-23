//! `sub/default`: pure text subscription.
//!
//! One URL per line, no extra headers. Every node in this file passed the Xray
//! probe and the speed floor.

use v2keylib::config::{self, Format};
use v2keylib::links;
use v2keylib::node::{Node, Protocol};

use crate::error::Result;

/// Whether an Xray config can carry this node.
pub fn representable(node: &Node) -> bool {
    config::unrepresentable(Format::XrayJson, node).is_none()
}

/// Render the file as pure text: one link per line, with announce comments.
pub fn render(nodes: &[Node], type_friendly: &str) -> Result<(String, usize)> {
    let mut eligible: Vec<&Node> = nodes.iter().filter(|node| representable(node)).collect();
    eligible.sort_by(|a, b| {
        rank(a.protocol)
            .cmp(&rank(b.protocol))
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.address.cmp(&b.address))
    });

    let mut body = super::headers::announce_comments(eligible.len(), type_friendly);
    let mut skipped = nodes.len().saturating_sub(eligible.len());
    for node in eligible {
        match links::render(node) {
            Ok(link) => {
                body.push_str(&link);
                body.push('\n');
            }
            Err(_) => skipped += 1,
        }
    }
    Ok((body, skipped))
}

fn rank(protocol: Protocol) -> u8 {
    match protocol {
        Protocol::Vless => 0,
        Protocol::Vmess => 1,
        Protocol::Trojan => 2,
        Protocol::Shadowsocks => 3,
        Protocol::Hysteria2 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use v2keylib::node::{Security, Transport};

    fn vless(address: &str) -> Node {
        let mut node = Node::new(Protocol::Vless, address, 443)
            .with_uuid("00000000-0000-0000-0000-000000000001");
        node.security = Security::Tls;
        node.sni = Some("example.com".into());
        node.transport = Transport::tcp();
        node.label = Some("NL-1".into());
        node
    }

    #[test]
    fn renders_pure_links_with_announce() {
        let vmess = Node::new(Protocol::Vmess, "192.0.2.2", 443)
            .with_uuid("00000000-0000-0000-0000-000000000002");
        let (body, _) = render(&[vmess, vless("192.0.2.1")], "полная подписка").unwrap();
        assert!(body.contains("# 🪨 Netherite Parser"));
        let links: Vec<&str> = body.lines().filter(|l| !l.starts_with('#')).collect();
        assert!(links[0].starts_with("vless://"));
    }

    #[test]
    fn empty_list_has_announce_only() {
        let (body, skipped) = render(&[], "полная подписка").unwrap();
        assert_eq!(skipped, 0);
        assert!(body.contains("# 🪨 Netherite Parser"));
        assert!(body.lines().all(|l| l.starts_with('#')));
    }
}
