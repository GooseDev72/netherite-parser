//! One error type for the whole run.
//!
//! A source that is down is not a fatal error — the run continues without it and
//! the report says so. The variants that are fatal are the ones about the local
//! machine: unreadable settings, unusable output directory, a missing probe
//! core. Those stop the run because continuing would produce a subscription
//! that looks fine and was never measured.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Config(String),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("bad source line {line}: {reason}")]
    Source { line: usize, reason: String },

    #[error("network: {0}")]
    Network(String),

    #[error("probe: {0}")]
    Probe(String),

    #[error("render: {0}")]
    Render(String),

    #[error(transparent)]
    V2key(#[from] v2keylib::error::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
