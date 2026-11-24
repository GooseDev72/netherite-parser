//! Batch probe runner. Verdicts are delegated to netherite-core evidence scoring.

use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::Semaphore;

use netherite_core::model::{Evidence, Stage, ValidationConfig as CoreConfig};
use netherite_core::scoring;

use crate::blacklist::{self, Blacklist, Resolved};
use crate::check::Core;
use crate::models::{Candidate, Checked, Status};
use crate::progress::Progress;
use crate::settings::Settings;

use super::probe::probe;

pub async fn run(
    candidates: Vec<Candidate>,
    core: Core,
    settings: &Settings,
    blacklist: Arc<Blacklist>,
    resolved: Arc<Resolved>,
    floor: f64,
) -> (Vec<Checked>, Arc<Progress>) {
    let progress = Arc::new(Progress::new(candidates.len(), settings.quiet));
    let concurrency = settings.concurrency.max(1);
    let speed_gate = Arc::new(Semaphore::new(settings.speed_concurrency.max(1)));

    let jobs = candidates.into_iter().map(|candidate| {
        let core = core.clone();
        let settings = settings.clone();
        let blacklist = Arc::clone(&blacklist);
        let resolved = Arc::clone(&resolved);
        let progress = Arc::clone(&progress);
        let speed_gate = Arc::clone(&speed_gate);
        async move {
            let checked = check_one(candidate, &core, &settings, &blacklist, &resolved, floor, &speed_gate).await;
            progress.record(&checked);
            checked
        }
    });

    let mut results: Vec<Checked> = futures::stream::iter(jobs)
        .buffer_unordered(concurrency)
        .collect()
        .await;
    results.sort_by_key(|checked| checked.node.strict_key());

    for _ in 0..settings.retry_dead {
        let pending: Vec<(usize, Candidate)> = results
            .iter()
            .enumerate()
            .filter(|(_, checked)| matches!(checked.status, Status::Dead { .. }))
            .map(|(index, checked)| (index, Candidate { node: checked.node.clone(), source: checked.source.clone() }))
            .collect();
        if pending.is_empty() { break; }

        progress.grow_total(pending.len());
        let attempts = pending.into_iter().map(|(index, candidate)| {
            let core = core.clone();
            let settings = settings.clone();
            let blacklist = Arc::clone(&blacklist);
            let resolved = Arc::clone(&resolved);
            let speed_gate = Arc::clone(&speed_gate);
            async move {
                let checked = check_one(candidate, &core, &settings, &blacklist, &resolved, floor, &speed_gate).await;
                (index, checked)
            }
        });
        let retried: Vec<(usize, Checked)> = futures::stream::iter(attempts)
            .buffer_unordered(concurrency)
            .collect()
            .await;
        for (index, checked) in retried {
            progress.retry(&results[index].status, &checked);
            results[index] = checked;
        }
    }

    (results, progress)
}

async fn check_one(
    candidate: Candidate,
    core: &Core,
    settings: &Settings,
    blacklist: &Blacklist,
    resolved: &Resolved,
    floor: f64,
    speed_gate: &Semaphore,
) -> Checked {
    let Candidate { node, source } = candidate;

    if let Some(hit) = blacklist::entry_hit(&Candidate { node: node.clone(), source: source.clone() }, blacklist, resolved) {
        return Checked {
            node, source, exit_ip: None, country: None, latency: None, mbps: None,
            status: Status::Blacklisted { address: hit.address, list_entry: hit.entry },
        };
    }

    let result = probe(&node, core, settings, speed_gate).await;
    let exit_ip = result.exit_ip();
    let country = result.country().map(str::to_string);

    // Build netherite-core evidence from real probe results
    let mut evidence = Vec::<Evidence>::new();

    // DNS/TCP are implicitly exercised by tunnel open+exit; record as evidence
    if result.answered() {
        evidence.push(Evidence::pass(Stage::TcpConnect, 20.0, "proxy tunnel connected").with_latency(result.latency.map(|d| d.as_millis()).unwrap_or(0)));
        evidence.push(Evidence::pass(Stage::DnsResolve, 15.0, "resolved via proxy/tunnel"));
    } else {
        evidence.push(Evidence::fail(Stage::TcpConnect, 20.0, result.error.clone().unwrap_or_else(|| "no exit answer".into())));
    }

    // Canary/exit evidence
    if let Some(ip) = exit_ip {
        evidence.push(Evidence::pass(Stage::HttpCanary, 30.0, format!("exit ip {ip}")).with_data("exit_ip", ip.to_string()));
    } else {
        evidence.push(Evidence::fail(Stage::HttpCanary, 30.0, "no exit ip"));
    }

    // Speed evidence
    match result.mbps {
        Some(mbps) if mbps >= floor => evidence.push(Evidence::pass(Stage::SpeedThreshold, 20.0, format!("{mbps:.2} Mbps >= {floor:.2}")).with_speed(mbps)),
        Some(mbps) => evidence.push(Evidence::fail(Stage::SpeedThreshold, 20.0, format!("{mbps:.2} Mbps < {floor:.2}")).with_speed(mbps)),
        None if settings.speed_enabled && result.answered() => evidence.push(Evidence::warn(Stage::SpeedThreshold, 20.0, 0.0, "speed not measured")),
        None => evidence.push(Evidence::skip(Stage::SpeedThreshold, "speed not measured")),
    }

    // Stability: single latency sample if present
    if let Some(lat) = result.latency {
        let ms = lat.as_millis();
        let ev = if ms <= settings.latency_timeout.as_millis() { Evidence::pass(Stage::Stability, 10.0, format!("latency {ms}ms")).with_latency(ms) } else { Evidence::warn(Stage::Stability, 10.0, 0.4, format!("latency {ms}ms")).with_latency(ms) };
        evidence.push(ev);
    }

    // Blacklist exit is a hard reject
    if let Some(hit) = blacklist::exit_hit(exit_ip, blacklist) {
        evidence.push(Evidence::fail(Stage::ContentIntegrity, 100.0, format!("blacklisted exit {} ({})", hit.address, hit.entry)));
    }

    let mut core_cfg = CoreConfig::default();
    core_cfg.speed_threshold_mbps = floor;
    core_cfg.require_speed = settings.speed_enabled;
    core_cfg.require_canary = true;
    core_cfg.require_dns = true;
    core_cfg.require_tcp = true;
    core_cfg.debug = false;

    let (verdict, _confidence) = scoring::classify(&evidence, &core_cfg);

    let status = if let Some(hit) = blacklist::exit_hit(exit_ip, blacklist) {
        Status::Blacklisted { address: hit.address, list_entry: hit.entry }
    } else {
        match verdict {
            netherite_core::model::Verdict::Working => Status::Published,
            netherite_core::model::Verdict::Degraded { reasons } => {
                if let Some(mbps) = result.mbps {
                    if mbps < floor { Status::TooSlow { mbps, floor } }
                    else if !settings.speed_enabled || result.answered() { Status::Dead { reason: reasons.join("; ") } }
                    else { Status::Dead { reason: reasons.join("; ") } }
                } else if result.answered() && !settings.speed_enabled {
                    Status::Published
                } else {
                    Status::Dead { reason: reasons.join("; ") }
                }
            }
            netherite_core::model::Verdict::Dead { reasons } => Status::Dead {
                reason: result.error.clone().unwrap_or_else(|| reasons.join("; ")),
            },
            netherite_core::model::Verdict::Rejected { reasons } => Status::Dead {
                reason: reasons.join("; "),
            },
        }
    };

    Checked { node, source, exit_ip, country, latency: result.latency, mbps: result.mbps, status }
}
