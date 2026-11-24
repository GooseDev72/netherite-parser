//! Stub: link counting (legacy), to be migrated to netherite-core.

use v2keylib::util::b64_decode;
const SCHEMES: &[&str] = &["vless://","vmess://","trojan://","trojan-go://","ss://","shadowsocks://","hysteria://","hysteria2://","hy2://","tuic://","anytls://","socks://","socks5://"];

pub fn count_links(body: &str) -> usize {
    let body = body.trim();
    if body.is_empty() { return 0; }
    if body.contains("://") { return count_lines(body); }
    if let Ok(decoded) = b64_decode(body) {
        if let Ok(text) = String::from_utf8(decoded) { return count_lines(&text); }
    }
    0
}

fn count_lines(text: &str) -> usize {
    text.lines().filter(|line| {
        let l = line.trim();
        SCHEMES.iter().any(|s| l.len() > s.len() && l.get(..s.len()).is_some_and(|h| h.eq_ignore_ascii_case(s)))
    }).count()
}
