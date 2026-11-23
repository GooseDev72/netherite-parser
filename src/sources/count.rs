//! Count link-shaped lines in a subscription body.
//!
//! `meta.json` distinguishes "configs seen" from "configs parsed". The parser
//! returns nodes and discards the lines it did not understand, so the count has
//! to be taken before parsing, from the body itself. It is deliberately a
//! lower-bound: a malformed line is not counted, a valid one is.

use v2keylib::util::b64_decode;

/// Schemes a subscription can carry. `hysteria2` is also written `hy2`.
const SCHEMES: &[&str] = &[
    "vless://",
    "vmess://",
    "trojan://",
    "trojan-go://",
    "ss://",
    "shadowsocks://",
    "hysteria://",
    "hysteria2://",
    "hy2://",
    "tuic://",
    "anytls://",
    "socks://",
    "socks5://",
];

pub fn count_links(body: &str) -> usize {
    let body = body.trim();
    if body.is_empty() {
        return 0;
    }
    if body.contains("://") {
        return count_lines(body);
    }
    // A base64-wrapped list is one long line and the only shape that reaches
    // here without a scheme. Decoding is cheap and bounded by the input.
    if let Ok(decoded) = b64_decode(body) {
        if let Ok(text) = String::from_utf8(decoded) {
            return count_lines(&text);
        }
    }
    0
}

fn count_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let line = line.trim();
            SCHEMES.iter().any(|scheme| {
                line.len() > scheme.len()
                    && line
                        .get(..scheme.len())
                        .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
            })
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_plain_lines() {
        let body = "vless://a\n# comment\nvmess://b\ntrojan://c\n";
        assert_eq!(count_links(body), 3);
    }

    #[test]
    fn counts_html_escaped_lines() {
        let body = "vless://a?x=1&amp;y=2\n";
        assert_eq!(count_links(body), 1);
    }

    #[test]
    fn counts_base64_body() {
        let body = v2keylib::util::b64_string(b"vless://a\nss://b\n");
        assert_eq!(count_links(&body), 2);
    }

    #[test]
    fn ignores_configs_and_noise() {
        assert_eq!(count_links("{\"outbounds\": []}"), 0);
        assert_eq!(count_links("<html><body>not a sub</body></html>"), 0);
    }
}
