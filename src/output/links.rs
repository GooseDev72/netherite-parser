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
                let link = fix_vmess_name(node, link);
                body.push_str(&link);
                body.push('\n');
            }
            Err(_) => skipped += 1,
        }
    }
    Ok((body, skipped))
}

fn fix_vmess_name(node: &Node, link: String) -> String {
    if node.protocol != Protocol::Vmess {
        return link;
    }
    let Some(label) = node.label.as_deref() else { return link; };
    let Some(b64) = link.strip_prefix("vmess://") else { return link; };
    let Ok(bytes) = v2keylib::util::b64_decode(b64.trim()) else { return link; };
    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return link; };
    if let Some(obj) = v.as_object_mut() {
        obj.insert("ps".to_string(), serde_json::Value::String(label.to_string()));
        obj.insert("remarks".to_string(), serde_json::Value::String(label.to_string()));
        if let Ok(out) = serde_json::to_vec(&v) {
            use base64::Engine;
            let enc = base64::engine::general_purpose::STANDARD.encode(out);
            return format!("vmess://{enc}");
        }
    }
    link
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
    fn vmess_name_in_ps() {
        let mut vmess = Node::new(Protocol::Vmess, "192.0.2.2", 443)
            .with_uuid("00000000-0000-0000-0000-000000000002");
        vmess.label = Some("NL-1234 Netherite".into());
        let (body, _) = render(&[vmess], "полная подписка").unwrap();
        let link = body.lines().find(|l| l.starts_with("vmess://")).unwrap();
        let b64 = link.strip_prefix("vmess://").unwrap();
        let json = String::from_utf8(v2keylib::util::b64_decode(b64).unwrap()).unwrap();
        eprintln!("{}", json);
        assert!(json.contains("NL-1234 Netherite"));
    }

    #[test]
    fn empty_list_has_announce_only() {
        let (body, skipped) = render(&[], "полная подписка").unwrap();
        assert_eq!(skipped, 0);
        assert!(body.contains("# 🪨 Netherite Parser"));
        assert!(body.lines().all(|l| l.starts_with('#')));
    }
}
