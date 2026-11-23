//! Probe one node through one core.
//!
//! The order is the same for every node and every failure has its own budget:
//! start the client, ask for the exit, time one request, download a burst. The
//! speed budget is separate on purpose — a slow node is not a dead node until the
//! floor says so, and a download that outlives the handshake must not be
//! reported as "never answered".

use std::time::Duration;

use tokio::sync::Semaphore;
use v2keylib::node::Node;
use v2keylib::tunnel::{Exit, Tunnel};

use crate::settings::Settings;

use super::Core;

/// What one probe learned.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub exit: Option<Exit>,
    pub latency: Option<Duration>,
    pub mbps: Option<f64>,
    pub error: Option<String>,
}

impl Probe {
    pub fn answered(&self) -> bool {
        self.exit.is_some()
    }

    pub fn exit_ip(&self) -> Option<std::net::IpAddr> {
        self.exit.as_ref().map(|exit| exit.ip)
    }

    pub fn country(&self) -> Option<&str> {
        self.exit.as_ref().and_then(|exit| exit.country.as_deref())
    }
}

/// Run one node.
///
/// `core` is shared by the batch; the process is per node, because one process
/// carrying one proxy is the only way to know which node answered.
/// `speed_gate` serializes the download phase: two simultaneous downloads share
/// the local line, so a 15 Mbit/s floor measured at concurrency ten would really
/// be a 15 Mbit/s floor per ten nodes.
pub async fn probe(node: &Node, core: &Core, settings: &Settings, speed_gate: &Semaphore) -> Probe {
    let open = tokio::time::timeout(settings.node_timeout, Tunnel::open(node, core.xray())).await;
    let tunnel = match open {
        Ok(Ok(tunnel)) => tunnel,
        Ok(Err(e)) => {
            return Probe {
                error: Some(plain(&e)),
                ..Default::default()
            }
        }
        Err(_) => {
            return Probe {
                error: Some(format!(
                    "{} did not start for {} within {:?}",
                    core.label(),
                    node.address,
                    settings.node_timeout
                )),
                ..Default::default()
            }
        }
    };

    let exit = tokio::time::timeout(settings.exit_timeout, tunnel.exit_ip()).await;
    let exit = match exit {
        Ok(Ok(exit)) => exit,
        Ok(Err(e)) => {
            return Probe {
                error: Some(format!("no exit answer: {}", plain(&e))),
                ..Default::default()
            }
        }
        Err(_) => {
            return Probe {
                error: Some(format!("no exit answer within {:?}", settings.exit_timeout)),
                ..Default::default()
            }
        }
    };

    let latency = tunnel
        .latency(
            &settings.latency_url,
            settings.speed_bytes.min(1_000_000),
            settings.latency_timeout,
        )
        .await;

    let mbps = if settings.speed_enabled {
        let _permit = speed_gate
            .acquire()
            .await
            .expect("speed semaphore is never closed");
        tunnel
            .speed(
                &settings.speed_urls,
                settings.speed_bytes,
                settings.speed_timeout,
            )
            .await
    } else {
        None
    };

    Probe {
        exit: Some(exit),
        latency,
        mbps,
        error: None,
    }
}

/// A library error without the "probe error:" prefix it adds on display.
///
/// The prefix is right when the error stands alone; here it sits after "no exit
/// answer:", where it reads as a stutter and pushes the actual service name off
/// the line.
fn plain(error: &v2keylib::error::Error) -> String {
    error
        .to_string()
        .trim_start_matches("probe error: ")
        .to_string()
}
