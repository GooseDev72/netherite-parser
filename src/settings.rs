//! Everything one run needs to know, in one struct.
//!
//! The settings are deliberately concrete rather than clever: paths, counts and
//! deadlines. The only derived value is the speed floor, and it is derived once
//! — see [`Settings::effective_threshold`] — because a threshold recomputed in
//! two places is how one of them ends up disagreeing with the report.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::Result;

/// The floor a published node has to clear.
///
/// 15 Mbit/s is enough for 1080p video and ordinary browsing, and it keeps a
/// node that works even when it is not fast. The builder's own link is measured
/// first, so a floor above what the line can do never empties the subscription.
pub const SPEED_FLOOR_MBPS: f64 = 15.0;

/// One short download that is still long enough to separate 25 from 35 Mbit/s.
pub const SPEED_BYTES: usize = 10_000_000;

/// The exit's own answer is cached by the API once per minute; this is the time
/// a handshake plus that answer gets before the node counts as unanswered.
pub const NODE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug)]
pub struct Settings {
    pub sources: PathBuf,
    pub tg: PathBuf,
    pub blacklist: PathBuf,
    pub out_dir: PathBuf,
    /// Stop after this many deduplicated candidates.
    pub max_nodes: Option<usize>,
    /// Keep at most this many candidates from one source (0 = all).
    pub per_source: usize,
    /// Fetch at most this many sources (0 = all).
    pub max_sources: usize,
    /// Probe processes alive at once, per core (Xray, then mihomo).
    pub concurrency: usize,
    /// Speed tests downloading at once. Above one, concurrent downloads share
    /// the local line and every node looks slower than it is.
    pub speed_concurrency: usize,
    /// Sources fetched at once.
    pub fetch_concurrency: usize,
    pub node_timeout: Duration,
    pub exit_timeout: Duration,
    pub latency_url: String,
    pub latency_timeout: Duration,
    pub speed_urls: Vec<String>,
    pub speed_timeout: Duration,
    pub speed_bytes: usize,
    pub threshold_mbps: f64,
    pub speed_enabled: bool,
    pub seed: u64,
    /// The name on the subscription and in every node name.
    pub name: String,
    /// Banner written as `#announce: <text>`; `None` for no line.
    pub announce: Option<String>,
    /// Fetch and merge only; no probe, no published files.
    pub dry_run: bool,
    /// Skip links already listed in `<out>/dead.txt`.
    pub resume: bool,
    /// How many extra attempts a node that did not answer gets. Off by
    /// default: a second pass over every dead candidate is a large increase in
    /// run time for a rescue that depends on how healthy the sources are that
    /// hour.
    pub retry_dead: usize,
    pub quiet: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sources: PathBuf::from("sources.txt"),
            tg: PathBuf::from("tg.txt"),
            blacklist: PathBuf::from("blacklist.txt"),
            out_dir: PathBuf::from("sub"),
            max_nodes: None,
            per_source: 0,
            max_sources: 0,
            concurrency: 6,
            speed_concurrency: 1,
            fetch_concurrency: 8,
            node_timeout: NODE_TIMEOUT,
            exit_timeout: NODE_TIMEOUT,
            // 204 with no body: the answer is the timing, and a body would be a
            // second, slower measurement of something else.
            latency_url: "https://www.gstatic.com/generate_204".into(),
            latency_timeout: Duration::from_secs(8),
            // Several on purpose: a single host that refuses a VPN exit (an
            // empty 200 from a speed-test endpoint does exactly that) would
            // otherwise make every node look unmeasured. Cloudflare first
            // because it was the one host that served a real body through the
            // exits this builder actually sees; the large-file mirrors are the
            // fallback for exits Cloudflare throttles.
            speed_urls: vec![
                "https://speed.cloudflare.com/__down?bytes=10000000".into(),
                "https://proof.ovh.net/files/10Mb.dat".into(),
                "https://ash-speed.hetzner.com/10MB.bin".into(),
            ],
            speed_timeout: Duration::from_secs(20),
            speed_bytes: SPEED_BYTES,
            threshold_mbps: SPEED_FLOOR_MBPS,
            speed_enabled: true,
            name: "Netherite".into(),
            announce: None,
            seed: 0,
            dry_run: false,
            resume: false,
            retry_dead: 0,
            quiet: false,
        }
    }
}

impl Settings {
    /// Read `configuration.toml` into settings.
    pub fn from_config(path: &Path) -> Result<Self> {
        Ok(crate::config::load(path)?.into_settings())
    }

    /// The speed floor this run actually applies.
    ///
    /// If the local link is slower than the floor, demanding 15 Mbit/s would
    /// throw away every node and call the result a healthy run. In that case the
    /// floor becomes three quarters of what the line itself can do: a node is
    /// still required to use most of the available bandwidth, and the report and
    /// `meta.json` carry the decision.
    pub fn effective_threshold(&self, baseline_mbps: Option<f64>) -> f64 {
        if !self.speed_enabled {
            return 0.0;
        }
        match baseline_mbps {
            Some(baseline) if baseline < self.threshold_mbps => (baseline * 0.75).max(1.0),
            _ => self.threshold_mbps,
        }
    }

    pub fn deadline(&self) -> Duration {
        // Enough for the handshake, the exit lookup, latency and a slow speed
        // test, with room for a node that times out at the last step.
        self.node_timeout + self.exit_timeout + self.latency_timeout + self.speed_timeout
    }

    pub fn dead_path(&self) -> PathBuf {
        self.out_dir.join("dead.txt")
    }

    pub fn report_path(&self) -> PathBuf {
        self.out_dir.join("source_report.txt")
    }
}
