//! Resolve entry hostnames once, so a blacklist can be applied before probing.
//!
//! A blacklist entry is an address; a node's entry point is often a name. The
//! conversion is DNS, it is slow, and doing it inside the check would make every
//! probe wait for a second resolver. One pass up front, bounded, cached by name.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;

/// A name-to-addresses cache for one run.
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    by_name: HashMap<String, Vec<IpAddr>>,
}

impl Resolved {
    pub fn get(&self, address: &str) -> Vec<IpAddr> {
        if let Ok(ip) = address.parse() {
            return vec![ip];
        }
        self.by_name.get(address).cloned().unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// Resolve every hostname that is not already an IP literal.
///
/// Failures are kept as an empty list rather than an error: a source full of
/// hostnames that no longer resolve should lose those nodes later, not stop the
/// run at the blacklist stage.
pub async fn resolve(addresses: &[String], concurrency: usize) -> Resolved {
    let mut wanted: Vec<String> = addresses
        .iter()
        .filter(|address| address.parse::<IpAddr>().is_err())
        .cloned()
        .collect();
    wanted.sort();
    wanted.dedup();
    if wanted.is_empty() {
        return Resolved::default();
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency.max(1)));
    let jobs = wanted.iter().cloned().map(|name| {
        let semaphore = Arc::clone(&semaphore);
        async move {
            let _permit = semaphore.acquire_owned().await.ok();
            // Port zero: `lookup_host` needs a port for its tuple, nothing more.
            let lookup = tokio::net::lookup_host((name.as_str(), 0u16));
            let ips = match tokio::time::timeout(Duration::from_secs(5), lookup).await {
                Ok(Ok(addrs)) => addrs.map(|addr| addr.ip()).collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            (name, ips)
        }
    });

    let by_name: HashMap<String, Vec<IpAddr>> = futures::stream::iter(jobs)
        .buffer_unordered(concurrency.max(1))
        .collect()
        .await;
    Resolved { by_name }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_bypass_the_cache() {
        let cache = Resolved::default();
        assert_eq!(
            cache.get("192.0.2.1"),
            vec!["192.0.2.1".parse::<IpAddr>().unwrap()]
        );
    }
}
