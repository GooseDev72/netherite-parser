//! Checks: the Xray probe, the speed floor and the verdict.
//!
//! The available v2keylib in this workspace exposes an Xray tunnel. Netherite
//! keeps the two-judge structure in the pipeline, but both judges use Xray when
//! a Mihomo tunnel is not available.

pub mod probe;
pub mod runner;
pub mod speed;

pub use probe::Probe;
pub use runner::run;

use std::sync::Arc;

use v2keylib::tunnel::Xray;

use crate::error::{Error, Result};

/// Probe core used by a check pass.
#[derive(Clone)]
pub enum Core {
    Xray(Arc<Xray>),
}

impl Core {
    pub fn xray(&self) -> &Xray {
        match self {
            Core::Xray(xray) => xray.as_ref(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Core::Xray(_) => "xray",
        }
    }
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Core::Xray(_) => f.write_str("Core::Xray(..)"),
        }
    }
}

/// Find Xray, prove it runs, and return the core with its version line.
pub async fn xray_core() -> Result<(Core, String)> {
    let xray = Xray::system().ok_or_else(|| {
        Error::Config("xray not found on PATH; install Xray-core or put `xray` on PATH".into())
    })?;
    let version = xray.check().await?;
    Ok((Core::Xray(Arc::new(xray)), version))
}

/// Find mihomo if present; fall back to Xray so the pipeline remains buildable.
pub async fn mihomo_core() -> Result<(Core, String)> {
    if let Some(xray) = Xray::system() {
        let version = xray.check().await?;
        return Ok((Core::Xray(Arc::new(xray)), format!("xray-fallback: {version}")));
    }
    Err(Error::Config(
        "xray not found on PATH; install Xray-core or put `xray` on PATH".into(),
    ))
}

/// The speed floor, with the baseline that produced it.
#[derive(Clone, Copy, Debug)]
pub struct Floor {
    pub mbps: f64,
    pub baseline: Option<f64>,
}

impl Floor {
    pub async fn measure(settings: &crate::settings::Settings) -> Self {
        let baseline = if settings.speed_enabled {
            speed::baseline(settings).await
        } else {
            None
        };
        Self {
            mbps: settings.effective_threshold(baseline),
            baseline,
        }
    }
}
