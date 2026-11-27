use regex::Regex;
use v2keylib::links;
use v2keylib::node::Node;

pub async fn scrape_channel(client: &reqwest::Client, channel: &str) -> crate::error::Result<Vec<Node>> {
    let ch = channel.trim().trim_start_matches('@');
    if ch.is_empty() { return Ok(Vec::new()); }
    let url = format!("https://t.me/s/{ch}");
    let resp = client.get(&url).send().await.map_err(|e| crate::error::Error::Config(e.to_string()))?;
    if !resp.status().is_success() { return Ok(Vec::new()); }
    let html = resp.text().await.map_err(|e| crate::error::Error::Config(e.to_string()))?;
    let html = super::channels::html_unescape(&html);

    let re = Regex::new(
        r#"(?i)(?:vless|vmess|trojan|ss|shadowsocks|hysteria2|hy2|tuic|socks5|socks|happ|v2raytun|incy)://[^\s"'<>\\]+"#,
    )
    .expect("valid regex");

    let mut nodes = Vec::new();
    for m in re.find_iter(&html) {
        let mut link = m.as_str().trim().trim_end_matches(['"', '\'', ',', ')', ']', '}']).to_string();
        if let Some(idx) = link.find("&quot;") { link.truncate(idx); }
        if link.is_empty() { continue; }
        let candidates = if let Some(decoded) = netherite_core::decode::decode_any(&link) {
            decoded.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>()
        } else {
            vec![link]
        };
        for c in candidates {
            if let Ok(node) = links::parse(&c) {
                nodes.push(node);
            }
        }
    }
    Ok(nodes)
}
