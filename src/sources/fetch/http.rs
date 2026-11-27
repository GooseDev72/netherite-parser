//! Download sources and parse nodes, with decode pass for happ/v2raytun/incy.

use std::time::{Duration, Instant};

use futures::StreamExt;
use v2keylib::links;

use crate::progress::FetchProgress;

use crate::sources::count::count_links;
use crate::sources::entry::SourceEntry;

#[derive(Clone, Debug)]
pub struct FetchedSource {
    pub entry: SourceEntry,
    pub nodes: Vec<v2keylib::node::Node>,
    pub raw_links: usize,
    pub error: Option<String>,
    pub took_ms: u128,
}

pub async fn fetch_all(
    entries: Vec<SourceEntry>,
    concurrency: usize,
    timeout: Duration,
    progress: &FetchProgress,
) -> Vec<FetchedSource> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("v2rayNG/1.9.0")
        .build()
        .expect("client");

    let jobs = entries.into_iter().enumerate().map(|(index, entry)| {
        let client = client.clone();
        async move {
            let started = Instant::now();
            let res = fetch_one(&client, &entry.url).await;
            progress.done(&entry.name);
            let took_ms = started.elapsed().as_millis();
            match res {
                Ok((nodes, raw_links)) => (index, entry, nodes, raw_links, None, took_ms),
                Err(e) => (index, entry, Vec::new(), 0, Some(e), took_ms),
            }
        }
    });

    let mut results: Vec<_> = futures::stream::iter(jobs)
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    results.sort_by_key(|(index, ..)| *index);

    results.into_iter().map(|(_, entry, nodes, raw_links, error, took_ms)| FetchedSource {
        entry, nodes, raw_links, error, took_ms
    }).collect()
}

async fn fetch_one(client: &reqwest::Client, url: &str) -> Result<(Vec<v2keylib::node::Node>, usize), String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let decoded = super::decode::decode_body(&body);
    let raw_links = count_links(&decoded);
    let mut nodes = Vec::new();
    for line in decoded.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') { continue; }
        // try decode encrypted wrapper per line
        let candidates = if let Some(d) = netherite_core::decode::decode_any(t) {
            d.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<String>>()
        } else { vec![t.to_string()] };
        for c in candidates {
            if let Ok(n) = links::parse(&c) { nodes.push(n); }
        }
    }
    Ok((nodes, raw_links))
}
