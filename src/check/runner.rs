//! The batch: every candidate through one core, bounded by concurrency.
//!
//! A check has four possible outcomes and the order in which they are decided is
//! the contract of the builder:
//!
//! * a blacklisted entry is dead without a probe,
//! * a node that never answers is dead,
//! * a node that answers from a blacklisted exit is dead,
//! * a node below the speed floor is measured but not published.
//!
//! Only the last one keeps its measurements in the report; the others are dead
//! and their links go to `dead.txt`.

use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::Semaphore;

use crate::check::Core;

use crate::blacklist::{self, Blacklist, Resolved};
use crate::models::{Candidate, Checked, Status};
use crate::progress::Progress;
use crate::settings::Settings;

use super::probe::probe;

/// Run the whole batch.
///
/// Returned in input order, with the progress counters, so the caller can report
/// both deterministically.
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
            let checked = check_one(
                candidate,
                &core,
                &settings,
                &blacklist,
                &resolved,
                floor,
                &speed_gate,
            )
            .await;
            progress.record(&checked);
            checked
        }
    });

    let mut results: Vec<Checked> = futures::stream::iter(jobs)
        .buffer_unordered(concurrency)
        .collect()
        .await;
    results.sort_by_key(|checked| {
        // Input order is not stored on `Checked`; sort by link identity for a
        // deterministic file and report. Order inside the subscription comes
        // later, from the published list.
        checked.node.strict_key()
    });

    // Public sources are flaky: a node that timed out once often answers on a
    // second attempt, and a first verdict of dead is not evidence enough to
    // throw it away. Only the dead are retried; a measured slow node stays slow,
    // and a blacklisted one is a decision, not a failure.
    for _ in 0..settings.retry_dead {
        let pending: Vec<(usize, Candidate)> = results
            .iter()
            .enumerate()
            .filter(|(_, checked)| matches!(checked.status, Status::Dead { .. }))
            .map(|(index, checked)| {
                (
                    index,
                    Candidate {
                        node: checked.node.clone(),
                        source: checked.source.clone(),
                    },
                )
            })
            .collect();
        if pending.is_empty() {
            break;
        }

        progress.grow_total(pending.len());
        let attempts = pending.into_iter().map(|(index, candidate)| {
            let core = core.clone();
            let settings = settings.clone();
            let blacklist = Arc::clone(&blacklist);
            let resolved = Arc::clone(&resolved);
            let speed_gate = Arc::clone(&speed_gate);

            async move {
                let checked = check_one(
                    candidate,
                    &core,
                    &settings,
                    &blacklist,
                    &resolved,
                    floor,
                    &speed_gate,
                )
                .await;
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

    if let Some(hit) = blacklist::entry_hit(
        &Candidate {
            node: node.clone(),
            source: source.clone(),
        },
        blacklist,
        resolved,
    ) {
        return Checked {
            node,
            source,
            exit_ip: None,
            country: None,
            latency: None,
            mbps: None,
            status: Status::Blacklisted {
                address: hit.address,
                list_entry: hit.entry,
            },
        };
    }

    let result = probe(&node, core, settings, speed_gate).await;
    let exit_ip = result.exit_ip();
    let country = result.country().map(str::to_string);

    let status = if let Some(hit) = blacklist::exit_hit(exit_ip, blacklist) {
        Status::Blacklisted {
            address: hit.address,
            list_entry: hit.entry,
        }
    } else if !result.answered() {
        Status::Dead {
            reason: result
                .error
                .clone()
                .unwrap_or_else(|| "no exit answer".into()),
        }
    } else if !settings.speed_enabled {
        Status::Published
    } else {
        match result.mbps {
            Some(mbps) if mbps >= floor => Status::Published,
            Some(mbps) => Status::TooSlow { mbps, floor },
            None => Status::Dead {
                reason: "exit answered, speed could not be measured".into(),
            },
        }
    };

    Checked {
        node,
        source,
        exit_ip,
        country,
        latency: result.latency,
        mbps: result.mbps,
        status,
    }
}
