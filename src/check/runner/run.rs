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

use crate::check::probe::probe;

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
            let checked = super::build::check_one(candidate, &core, &settings, &blacklist, &resolved, floor, &speed_gate).await;
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
                let checked = super::build::check_one(candidate, &core, &settings, &blacklist, &resolved, floor, &speed_gate).await;
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
