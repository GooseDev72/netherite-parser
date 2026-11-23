//! Download each source on its own.
//!
//! The library can fetch many URLs at once, but that loses the one thing this
//! builder wants from a source list: which URL produced which node. Fetching
//! separately costs a client per request and buys per-source accounting, which
//! is the point of [`crate::sources::report`].

use std::time::{Duration, Instant};

use futures::StreamExt;
use v2keylib::subscription::{self, FetchOptions};

use crate::progress::FetchProgress;

use super::count::count_links;
use super::entry::SourceEntry;

/// One source after the network stage.
#[derive(Clone, Debug)]
pub struct FetchedSource {
    pub entry: SourceEntry,
    pub nodes: Vec<v2keylib::node::Node>,
    /// Link-shaped lines the body contained, before parsing.
    pub raw_links: usize,
    pub error: Option<String>,
    pub took_ms: u128,
}

/// Fetch sources with a bounded number of requests in flight.
///
/// Results keep the registry's order, not completion order, because a report
/// sorted by "who answered first" is a report of the network, not of the
/// sources.
pub async fn fetch_all(
    entries: Vec<SourceEntry>,
    concurrency: usize,
    timeout: Duration,
    progress: &FetchProgress,
) -> Vec<FetchedSource> {
    let options = FetchOptions {
        timeout,
        user_agent: "v2rayNG/1.9.0".into(),
        ..Default::default()
    };

    let jobs = entries.into_iter().enumerate().map(|(index, entry)| {
        let options = options.clone();
        async move {
            let started = Instant::now();
            let fetched = subscription::fetch(&entry.url, &options).await;
            progress.done(&entry.name);
            let took_ms = started.elapsed().as_millis();
            match fetched {
                Ok(sub) => {
                    let nodes = sub.nodes();
                    let raw_links = nodes.len();
                    (index, entry, nodes, raw_links, None, took_ms)
                }
                Err(e) => (index, entry, Vec::new(), 0, Some(e.to_string()), took_ms),
            }
        }
    });

    let mut results: Vec<_> = futures::stream::iter(jobs)
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    results.sort_by_key(|(index, ..)| *index);

    results
        .into_iter()
        .map(
            |(_, entry, nodes, raw_links, error, took_ms)| FetchedSource {
                entry,
                nodes,
                raw_links,
                error,
                took_ms,
            },
        )
        .collect()
}

/// Count link-shaped lines in a body, exposed for the report and tests.
pub fn count_from_body(body: &str) -> usize {
    count_links(body)
}
