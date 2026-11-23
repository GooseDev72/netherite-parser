//! Telegram public channel scraper.
//!
//! Reads a `tg.txt` file (one channel per line, URL or @name), fetches
//! `https://t.me/s/<channel>` and extracts proxy links. No whitelist or hosting
//! checks are applied.

use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
use regex::Regex;
use v2keylib::links;
use v2keylib::node::Node;

use crate::error::Result;
use crate::util::read_text;

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Load channel names from a tg.txt file. Missing file => empty list.
pub fn load_channels(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = read_text(path)?;
    let mut channels = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name = line
            .strip_prefix("https://t.me/s/")
            .or_else(|| line.strip_prefix("http://t.me/s/"))
            .or_else(|| line.strip_prefix("https://t.me/"))
            .or_else(|| line.strip_prefix("http://t.me/"))
            .or_else(|| line.strip_prefix('@'))
            .unwrap_or(line);
        let name = name.trim().trim_matches('/');
        if !name.is_empty() {
            channels.push(name.to_string());
        }
    }
    channels.sort();
    channels.dedup();
    Ok(channels)
}

/// Scrape channels and return parsed nodes.
pub async fn scrape_channels(channels: &[String], concurrency: usize, timeout: Duration) -> Vec<Node> {
    if channels.is_empty() {
        return Vec::new();
    }
    let client = match reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let re = Regex::new(
        r#"(?i)(?:vless|vmess|trojan|ss|shadowsocks|hysteria2|hy2|tuic|socks5|socks)://[^\s"'<>\\]+"#,
    )
    .expect("valid regex");

    let jobs = channels.iter().cloned().map(|ch| {
        let client = client.clone();
        let re = re.clone();
        async move { scrape_one(&client, &re, &ch).await }
    });

    let results: Vec<Vec<Node>> = futures::stream::iter(jobs)
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;

    let mut out = Vec::new();
    for nodes in results {
        out.extend(nodes);
    }
    out
}

async fn scrape_one(client: &reqwest::Client, re: &Regex, channel: &str) -> Vec<Node> {
    let url = format!("https://t.me/s/{channel}");
    let body = match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    let html = html_unescape(&body);
    let mut nodes = Vec::new();
    for m in re.find_iter(&html) {
        let mut link = m.as_str().trim().trim_end_matches(['"', '\'', ',', ')', ']', '}']).to_string();
        // strip trailing html entities/noise
        if let Some(idx) = link.find("&quot;") { link.truncate(idx); }
        if link.is_empty() { continue; }
        if let Ok(node) = links::parse(&link) {
            nodes.push(node);
        }
    }
    nodes
}

fn html_unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
