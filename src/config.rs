//! `configuration.toml`: limits, thresholds and identity in one file.
//!
//! The command line still wins. A flag given on the command line overrides the
//! file, and the file overrides the compiled defaults, so a run can be tuned by
//! editing one file and an experiment can still be a single flag.
//!
//! Missing keys fall back to the defaults, unknown keys are an error: a typo in
//! `speed_mbps` that silently does nothing is worse than a run that refuses to
//! start.

use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::settings::Settings;

/// The whole file.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub limits: Limits,
    pub thresholds: Thresholds,
    pub identity: Identity,
}

/// How much work a run does at once, and how much of it.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Stop after this many deduplicated candidates; 0 = no cap.
    pub max_nodes: usize,
    /// Keep at most this many candidates per source; 0 = all.
    pub per_source: usize,
    /// Fetch at most this many sources; 0 = all.
    pub max_sources: usize,
    /// Probe processes alive at once, per core (Xray, then mihomo).
    pub concurrency: usize,
    /// Speed downloads at once.
    pub speed_concurrency: usize,
    /// Sources fetched at once.
    pub fetch_concurrency: usize,
    /// Extra attempts for a node that did not answer.
    pub retry_dead: usize,
}

impl Default for Limits {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            max_nodes: settings.max_nodes.unwrap_or(0),
            per_source: settings.per_source,
            max_sources: settings.max_sources,
            concurrency: settings.concurrency,
            speed_concurrency: settings.speed_concurrency,
            fetch_concurrency: settings.fetch_concurrency,
            retry_dead: settings.retry_dead,
        }
    }
}

/// What has to be measured for a node to be published.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    /// The speed floor, in Mbit/s.
    pub speed_mbps: f64,
    /// Handshake and exit lookup budget, in seconds.
    pub node_timeout_secs: u64,
    /// Exit lookup budget on its own, in seconds.
    pub exit_timeout_secs: u64,
    /// Per-URL download budget, in seconds.
    pub speed_timeout_secs: u64,
    /// Bytes to ask for in the speed test.
    pub speed_bytes: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            speed_mbps: settings.threshold_mbps,
            node_timeout_secs: settings.node_timeout.as_secs(),
            exit_timeout_secs: settings.exit_timeout.as_secs(),
            speed_timeout_secs: settings.speed_timeout.as_secs(),
            speed_bytes: settings.speed_bytes,
        }
    }
}

/// The name on the subscription and the banner above it.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Identity {
    /// Used in the profile title and in every node name.
    pub name: String,
    /// Written as `#announce: <text>` after the three profile headers; empty
    /// for no announce line.
    pub announce: String,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            name: "Netherite".into(),
            announce: String::new(),
        }
    }
}

impl Configuration {
    /// Fold the file into a [`Settings`], keeping the defaults for keys the file
    /// does not mention.
    pub fn into_settings(self) -> Settings {
        let mut settings = Settings::default();
        let limits = self.limits;
        settings.max_nodes = (limits.max_nodes > 0).then_some(limits.max_nodes);
        settings.per_source = limits.per_source;
        settings.max_sources = limits.max_sources;
        if limits.concurrency > 0 {
            settings.concurrency = limits.concurrency;
        }
        if limits.speed_concurrency > 0 {
            settings.speed_concurrency = limits.speed_concurrency;
        }
        if limits.fetch_concurrency > 0 {
            settings.fetch_concurrency = limits.fetch_concurrency;
        }
        settings.retry_dead = limits.retry_dead;

        let thresholds = self.thresholds;
        if thresholds.speed_mbps > 0.0 {
            settings.threshold_mbps = thresholds.speed_mbps;
        }
        if thresholds.node_timeout_secs > 0 {
            settings.node_timeout = std::time::Duration::from_secs(thresholds.node_timeout_secs);
        }
        if thresholds.exit_timeout_secs > 0 {
            settings.exit_timeout = std::time::Duration::from_secs(thresholds.exit_timeout_secs);
        }
        if thresholds.speed_timeout_secs > 0 {
            settings.speed_timeout = std::time::Duration::from_secs(thresholds.speed_timeout_secs);
        }
        if thresholds.speed_bytes > 0 {
            settings.speed_bytes = thresholds.speed_bytes;
        }

        let name = self.identity.name.trim();
        if !name.is_empty() {
            settings.name = name.to_string();
        }
        let announce = self.identity.announce.trim();
        settings.announce = (!announce.is_empty()).then(|| announce.to_string());

        settings
    }
}

/// Read and parse the file.
pub fn load(path: &Path) -> Result<Configuration> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    toml::from_str(&text).map_err(|e| Error::Config(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_defaults() {
        let settings: Settings = toml::from_str::<Configuration>("").unwrap().into_settings();
        let default = Settings::default();
        assert_eq!(settings.threshold_mbps, default.threshold_mbps);
        assert_eq!(settings.concurrency, default.concurrency);
        assert_eq!(settings.name, "Netherite");
        assert_eq!(settings.announce, None);
    }

    #[test]
    fn a_partial_file_keeps_the_rest() {
        let text = r#"
            [thresholds]
            speed_mbps = 7.5

            [identity]
            name = "Rock"
            announce = "hello"
        "#;
        let settings = toml::from_str::<Configuration>(text)
            .unwrap()
            .into_settings();
        assert_eq!(settings.threshold_mbps, 7.5);
        assert_eq!(settings.concurrency, Settings::default().concurrency);
        assert_eq!(settings.name, "Rock");
        assert_eq!(settings.announce.as_deref(), Some("hello"));
    }

    #[test]
    fn zero_stands_for_unset_in_limits_and_timeouts() {
        let text = r#"
            [limits]
            max_nodes = 0
            concurrency = 0
            retry_dead = 0

            [thresholds]
            node_timeout_secs = 0
        "#;
        let settings = toml::from_str::<Configuration>(text)
            .unwrap()
            .into_settings();
        assert_eq!(settings.max_nodes, None);
        assert_eq!(settings.concurrency, Settings::default().concurrency);
        assert_eq!(settings.node_timeout, Settings::default().node_timeout);
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_default() {
        let text = "[thresholds]\nspeed_mbits = 5\n";
        assert!(toml::from_str::<Configuration>(text).is_err());
    }
}
