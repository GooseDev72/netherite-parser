//! The data that moves between stages.
//!
//! A run has three lists and never mixes them: candidates (what was fetched),
//! checked nodes (what the cores said), and published nodes (what cleared the
//! floor and the blacklist). Keeping them as separate types is what makes
//! `total_cfg`, `total_merged_cfg` and `total_dead_cfg` in `meta.json`
//! unambiguous instead of three interpretations of one vector.

use std::net::IpAddr;
use std::time::Duration;

use v2keylib::node::{Node, Protocol};

/// A node and the source that introduced it.
///
/// The source stays attached through deduplication: when the same endpoint
/// appears in five lists, the first one is kept, and the report credits that
/// one. That is the honest attribution — the others did not add a node, they
/// repeated one.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub node: Node,
    pub source: String,
}

/// Why a node is not in the subscription.
#[derive(Clone, Debug)]
pub enum Status {
    /// Answered through a core and cleared the floor.
    Published,
    /// Entry or exit address is in the blacklist.
    Blacklisted { address: IpAddr, list_entry: String },
    /// Never completed a request.
    Dead { reason: String },
    /// Worked, but slower than the floor.
    TooSlow { mbps: f64, floor: f64 },
}

impl Status {
    pub fn published(&self) -> bool {
        matches!(self, Status::Published)
    }

    pub fn dead(&self) -> bool {
        matches!(self, Status::Dead { .. } | Status::Blacklisted { .. })
    }

    /// One line for `dead.txt`'s companion report and the progress log.
    pub fn reason(&self) -> String {
        match self {
            Status::Published => "ok".into(),
            Status::Blacklisted {
                address,
                list_entry,
            } => {
                format!("blacklisted {address} ({list_entry})")
            }
            Status::Dead { reason } => reason.clone(),
            Status::TooSlow { mbps, floor } => {
                format!("{mbps:.1} Mbit/s < {floor:.1} Mbit/s")
            }
        }
    }
}

/// A node after one core's check.
#[derive(Clone, Debug)]
pub struct Checked {
    pub node: Node,
    pub source: String,
    /// Where the traffic actually left, when the node answered.
    pub exit_ip: Option<IpAddr>,
    /// ISO 3166-1 alpha-2 code from the exit lookup.
    pub country: Option<String>,
    pub latency: Option<Duration>,
    /// Measured download speed in megabits per second.
    pub mbps: Option<f64>,
    pub status: Status,
}

impl Checked {
    pub fn is_published(&self) -> bool {
        self.status.published()
    }
}

/// What one source contributed.
#[derive(Clone, Debug, Default)]
pub struct SourceStat {
    pub name: String,
    pub url: String,
    /// Link-shaped lines found in the body, before parsing.
    pub raw_links: usize,
    /// Nodes the parser understood.
    pub parsed: usize,
    /// Nodes that survived deduplication and are credited to this source.
    pub unique: usize,
    /// Nodes that answered through either core, regardless of speed.
    pub alive: usize,
    /// Nodes that are in at least one published subscription.
    pub passed: usize,
    pub error: Option<String>,
    /// Wall time spent fetching and parsing.
    pub fetch_ms: u128,
}

impl SourceStat {
    /// How much of this source was worth keeping, as a percentage.
    pub fn yield_percent(&self) -> f64 {
        if self.unique == 0 {
            return 0.0;
        }
        self.passed as f64 * 100.0 / self.unique as f64
    }

    pub fn usable(&self) -> bool {
        self.error.is_none() && self.parsed > 0
    }
}

/// Protocol totals for `meta.json`.
#[derive(Clone, Debug, Default)]
pub struct ProtocolCounts {
    pub vless: usize,
    pub ss: usize,
    pub trojan: usize,
    pub vmess: usize,
    pub hy2: usize,
    pub other: usize,
}

impl ProtocolCounts {
    pub fn of(nodes: &[Node]) -> Self {
        let mut counts = Self::default();
        for node in nodes {
            match node.protocol {
                Protocol::Vless => counts.vless += 1,
                Protocol::Vmess => counts.vmess += 1,
                Protocol::Trojan => counts.trojan += 1,
                Protocol::Shadowsocks => counts.ss += 1,
                Protocol::Hysteria2 => counts.hy2 += 1,
                _ => counts.other += 1,
            }
        }
        counts
    }

    pub fn total(&self) -> usize {
        self.vless + self.ss + self.trojan + self.vmess + self.hy2 + self.other
    }

    /// The `mihomo_compatible` / `xray_compatible` object.
    ///
    /// The five protocols the parser knows are always present, so a reader can
    /// index a key without checking for its absence. `other` appears only when
    /// it is non-zero: it is not a protocol, just the category for anything that
    /// is not one of the five.
    pub fn as_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("vless".into(), self.vless.into());
        map.insert("vmess".into(), self.vmess.into());
        map.insert("trojan".into(), self.trojan.into());
        map.insert("ss".into(), self.ss.into());
        map.insert("hy2".into(), self.hy2.into());
        if self.other > 0 {
            map.insert("other".into(), self.other.into());
        }
        serde_json::Value::Object(map)
    }
}

/// Everything the finished run knows, for `meta.json` and the console summary.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub mihomo_version: String,
    pub xray_version: String,
    pub total_cfg: usize,
    pub total_parsed_cfg: usize,
    pub total_merged_cfg: usize,
    pub total_dead_cfg: usize,
    pub total_blacklisted_cfg: usize,
    pub total_too_slow_cfg: usize,
    /// Nodes published in at least one subscription.
    pub published_total: usize,
    /// Nodes published in both subscriptions.
    pub published_both: usize,
    /// Nodes that passed mihomo, for `sub/mihomo`.
    pub mihomo_nodes: Vec<Node>,
    /// Nodes that passed Xray, for `sub/default`.
    pub xray_nodes: Vec<Node>,
    pub mihomo_counts: ProtocolCounts,
    pub xray_counts: ProtocolCounts,
    pub baseline_mbps: Option<f64>,
    pub threshold_mbps: f64,
    pub started_at: std::time::SystemTime,
    pub elapsed: Duration,
}

impl Outcome {
    pub fn total_xray_cfg(&self) -> usize {
        self.xray_nodes.len()
    }

    pub fn total_mihomo_cfg(&self) -> usize {
        self.mihomo_nodes.len()
    }
}
