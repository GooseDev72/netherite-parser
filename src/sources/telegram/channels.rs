//! Telegram public channel scraper.
//!
//! Reads a `tg.txt` file (one channel per line, URL or @name), fetches
//! `https://t.me/s/<channel>` and extracts proxy links. No whitelist or hosting
//! checks are applied.

use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
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
pub async fn scrape_channels(channels: &[String], concurrency: usize, timeout: Duration, quiet: bool) -> Vec<Node> {
    let client = match reqwest::Client::builder().timeout(timeout).user_agent(USER_AGENT).build() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let total = channels.len();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let jobs = channels.iter().cloned().map(|ch| {
        let client = client.clone();
        let done = done.clone();
        async move {
            let r = super::scrape::scrape_channel(&client, &ch).await.unwrap_or_default();
            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if !quiet && (n % 10 == 0 || n == total) {
                eprintln!("telegram: scraped {n}/{total} channels");
            }
            r
        }
    });
    let mut nodes: Vec<Node> = futures::stream::iter(jobs)
        .buffer_unordered(concurrency.max(1))
        .flat_map(futures::stream::iter)
        .collect()
        .await;
    let mut seen = std::collections::HashSet::new();
    nodes.retain(|n| seen.insert(n.strict_key()));
    if !quiet {
        eprintln!("telegram: {} unique nodes from {} channels", nodes.len(), total);
    }
    nodes
}

pub fn html_unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
