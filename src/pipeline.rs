//! The order of a run, from the source list to the five output files.
//!
//! Each stage is one function so a failure names the stage it came from. The
//! pipeline itself holds no policy: the floor is in [`crate::check::speed`], the
//! blacklist in [`crate::blacklist`], the names in [`crate::geo::naming`].

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use v2keylib::node::Node;

use crate::blacklist::{self, Blacklist};
use crate::check::{self, Floor};
use crate::collect::{self, Merged};
use crate::error::{Error, Result};
use crate::geo::published_name;
use crate::models::{Checked, Outcome, ProtocolCounts, SourceStat, Status};
use crate::output::{self, Files};
use crate::progress::FetchProgress;
use crate::settings::Settings;
use crate::sources::{self, FetchedSource, Registry};
use crate::util::{human_mbps, Ids};

/// A finished, published run.
pub struct RunReport {
    pub outcome: Outcome,
    pub stats: Vec<SourceStat>,
    pub files: Files,
    pub sources_total: usize,
    pub sources_used: usize,
    pub duplicates: usize,
    pub truncated: usize,
}

/// Run the builder.
///
/// `Ok(None)` means `--dry-run`: the sources were fetched and merged, but nothing
/// was probed and nothing was written.
pub async fn run(settings: Settings) -> Result<Option<RunReport>> {
    let started = Instant::now();
    let started_at = SystemTime::now();

    let registry = Registry::load(&settings.sources)?;
    if registry.is_empty() {
        return Err(Error::Config(format!(
            "{} lists no sources",
            settings.sources.display()
        )));
    }
    let entries = registry.take_all_or(settings.max_sources);
    if !settings.quiet {
        eprintln!(
            "fetching {} source(s) from {}",
            entries.len(),
            settings.sources.display()
        );
    }

    let fetch_progress = FetchProgress::new(entries.len(), settings.quiet);
    let mut fetched = sources::fetch::fetch_all(
        entries,
        settings.fetch_concurrency,
        Duration::from_secs(30),
        &fetch_progress,
    )
    .await;
    fetch_progress.finish();

    // Telegram scraper (ANT1VEN0M-style public t.me/s/ scrape), no whitelist/hosting checks.
    let tg_channels = sources::telegram::load_channels(&settings.tg).unwrap_or_default();
    if !tg_channels.is_empty() {
        if !settings.quiet {
            eprintln!("scraping {} telegram channel(s) from {}", tg_channels.len(), settings.tg.display());
        }
        let started = Instant::now();
        let nodes = sources::telegram::scrape_channels(&tg_channels, settings.fetch_concurrency, Duration::from_secs(15)).await;
        let took_ms = started.elapsed().as_millis();
        let raw_links = nodes.len();
        fetched.push(FetchedSource {
            entry: sources::SourceEntry {
                name: "telegram".into(),
                url: settings.tg.display().to_string(),
            },
            nodes,
            raw_links,
            error: None,
            took_ms,
        });
    }

    let sources_used = fetched
        .iter()
        .filter(|source| source.error.is_none() && !source.nodes.is_empty())
        .count();

    let mut merged = collect::merge(&fetched, settings.per_source, settings.max_nodes);

    // A previous run's dead.txt is an optimisation, not a truth: the flag has to
    // be asked for, and the comparison is the strict identity so a renamed link
    // is still the same node.
    if settings.resume {
        let known = output::dead::load_known(&settings.dead_path());
        if !known.is_empty() {
            let before = merged.candidates.len();
            merged
                .candidates
                .retain(|candidate| !known.contains(&candidate.node.strict_key()));
            if !settings.quiet {
                eprintln!(
                    "resume: skipped {} link(s) from {}",
                    before - merged.candidates.len(),
                    settings.dead_path().display()
                );
            }
            recompute_credited(&mut merged);
        }
    }

    if settings.dry_run {
        let report = sources::Report { stats: &merged.stats }.render();
        let _ = crate::util::write_text(&settings.report_path(), &report);
        print_dry_run(&settings, &fetched, &merged, sources_used);
        return Ok(None);
    }

    let (xray, xray_version) = check::xray_core().await?;
    let (mihomo, mihomo_version) = check::mihomo_core().await?;
    if !settings.quiet {
        eprintln!("xray:   {xray_version}");
        eprintln!("mihomo: {mihomo_version}");
    }

    let floor = Floor::measure(&settings).await;
    if !settings.quiet {
        match floor.baseline {
            Some(mbps) => eprintln!(
                "local line: {} Mbit/s; floor: {} Mbit/s",
                human_mbps(mbps),
                human_mbps(floor.mbps)
            ),
            None if settings.speed_enabled => {
                eprintln!(
                    "local line: not measured; floor stays at {} Mbit/s",
                    human_mbps(floor.mbps)
                );
            }
            None => eprintln!("speed filter disabled"),
        }
    }

    let blacklist =
        Arc::new(Blacklist::load(&settings.blacklist).map_err(|e| {
            Error::Config(format!("blacklist {}: {e}", settings.blacklist.display()))
        })?);
    if blacklist.is_empty() && !settings.quiet {
        eprintln!(
            "blacklist: {} not found or empty; every address is eligible",
            settings.blacklist.display()
        );
    }
    let addresses: Vec<String> = merged
        .candidates
        .iter()
        .map(|candidate| candidate.node.address.clone())
        .collect();
    let resolved = Arc::new(if blacklist.is_empty() {
        blacklist::Resolved::default()
    } else {
        blacklist::resolve::resolve(&addresses, 32).await
    });

    if !settings.quiet {
        eprintln!(
            "checking {} candidate(s) through xray, {} at a time",
            merged.candidates.len(),
            settings.concurrency
        );
    }
    let (xray_checked, xray_progress) = check::run(
        merged.candidates.clone(),
        xray,
        &settings,
        Arc::clone(&blacklist),
        Arc::clone(&resolved),
        floor.mbps,
    )
    .await;
    xray_progress.finish();

    if !settings.quiet {
        eprintln!(
            "checking {} candidate(s) through mihomo, {} at a time",
            merged.candidates.len(),
            settings.concurrency
        );
    }
    let (mihomo_checked, mihomo_progress) = check::run(
        merged.candidates.clone(),
        mihomo,
        &settings,
        blacklist,
        resolved,
        floor.mbps,
    )
    .await;
    mihomo_progress.finish();

    // One ledger for `dead.txt` and the report: a node is published if either
    // core published it, and blacklisted if either core saw a blacklisted exit.
    let combined = merge_checked(&mihomo_checked, &xray_checked);

    let mut mihomo_published: Vec<Checked> = mihomo_checked
        .iter()
        .filter(|checked| checked.is_published())
        .cloned()
        .collect();
    let mut xray_published: Vec<Checked> = xray_checked
        .iter()
        .filter(|checked| checked.is_published())
        .cloned()
        .collect();

    // A blacklist hit in either core is final: the same config must not appear
    // in one subscription while the other saw a forbidden exit.
    let blocked: HashSet<String> = combined
        .iter()
        .filter(|checked| matches!(checked.status, Status::Blacklisted { .. }))
        .map(|checked| checked.node.strict_key())
        .collect();
    mihomo_published.retain(|checked| !blocked.contains(&checked.node.strict_key()));
    xray_published.retain(|checked| !blocked.contains(&checked.node.strict_key()));

    name_published(&mut mihomo_published, settings.seed, &settings.name);
    name_published(&mut xray_published, settings.seed, &settings.name);

    let mut mihomo_nodes: Vec<Node> = mihomo_published.iter().map(|c| c.node.clone()).collect();
    mihomo_nodes.sort_by(|a, b| a.label.cmp(&b.label));
    let mut xray_nodes: Vec<Node> = xray_published.iter().map(|c| c.node.clone()).collect();
    xray_nodes.sort_by(|a, b| a.label.cmp(&b.label));

    let mihomo_counts = ProtocolCounts::of(&mihomo_nodes);
    let xray_counts = ProtocolCounts::of(&xray_nodes);

    let published_total = combined.iter().filter(|c| c.is_published()).count();
    let mihomo_keys: HashSet<String> = mihomo_nodes.iter().map(|node| node.strict_key()).collect();
    let published_both = xray_nodes
        .iter()
        .filter(|node| mihomo_keys.contains(&node.strict_key()))
        .count();

    // What is credited in the report has to match what was probed.
    let mut stats = merged.stats.clone();
    let alive_by_source = count_by_source(&combined, |checked| checked.exit_ip.is_some());
    let passed_by_source = count_by_source(&combined, |checked| checked.is_published());
    for stat in &mut stats {
        stat.alive = alive_by_source
            .get(stat.name.as_str())
            .copied()
            .unwrap_or(0);
        stat.passed = passed_by_source
            .get(stat.name.as_str())
            .copied()
            .unwrap_or(0);
    }

    let total_merged = merged.candidates.len();
    let outcome = Outcome {
        mihomo_version,
        xray_version,
        total_cfg: merged.total_cfg,
        total_parsed_cfg: merged.total_parsed_cfg,
        total_merged_cfg: total_merged,
        total_dead_cfg: total_merged.saturating_sub(published_total),
        total_blacklisted_cfg: combined
            .iter()
            .filter(|c| matches!(c.status, Status::Blacklisted { .. }))
            .count(),
        total_too_slow_cfg: combined
            .iter()
            .filter(|c| matches!(c.status, Status::TooSlow { .. }))
            .count(),
        published_total,
        published_both,
        mihomo_nodes,
        xray_nodes,
        mihomo_counts,
        xray_counts,
        baseline_mbps: floor.baseline,
        threshold_mbps: floor.mbps,
        started_at,
        elapsed: started.elapsed(),
    };

    let files = output::write_all(&settings, &outcome, &combined, &stats)?;

    Ok(Some(RunReport {
        outcome,
        stats,
        files,
        sources_total: fetched.len(),
        sources_used,
        duplicates: merged.duplicates,
        truncated: merged.truncated,
    }))
}

/// Fold the two core verdicts into one ledger for `dead.txt` and the report.
///
/// A node is published when either core published it. A blacklist hit in either
/// core wins over a publication in the other: the brief says a forbidden exit
/// is never published, and that must not depend on which client noticed it.
/// Otherwise the node is too slow when either core measured it below the floor,
/// and dead when neither core got an answer.
fn merge_checked(mihomo: &[Checked], xray: &[Checked]) -> Vec<Checked> {
    use std::collections::HashMap;

    let xray_by_key: HashMap<String, &Checked> = xray
        .iter()
        .map(|checked| (checked.node.strict_key(), checked))
        .collect();
    let mut merged: Vec<Checked> = mihomo
        .iter()
        .map(|checked| {
            combine(
                checked,
                xray_by_key.get(&checked.node.strict_key()).copied(),
            )
        })
        .collect();
    merged.sort_by_key(|checked| checked.node.strict_key());
    merged
}

/// The combined verdict for one node.
fn combine(mihomo: &Checked, xray: Option<&Checked>) -> Checked {
    let mihomo_black = matches!(mihomo.status, Status::Blacklisted { .. });
    let xray_black = xray
        .map(|checked| matches!(checked.status, Status::Blacklisted { .. }))
        .unwrap_or(false);
    if mihomo_black || xray_black {
        let source = if mihomo_black {
            mihomo
        } else {
            xray.expect("xray verdict exists")
        };
        return rebase(mihomo, source, source.status.clone());
    }

    let mihomo_ok = mihomo.is_published();
    let xray_ok = xray.map(|checked| checked.is_published()).unwrap_or(false);
    if mihomo_ok || xray_ok {
        let source = if mihomo_ok {
            mihomo
        } else {
            xray.expect("xray verdict exists")
        };
        return rebase(mihomo, source, Status::Published);
    }

    if matches!(mihomo.status, Status::TooSlow { .. }) {
        return rebase(mihomo, mihomo, mihomo.status.clone());
    }
    if let Some(xray) = xray {
        if matches!(xray.status, Status::TooSlow { .. }) {
            return rebase(mihomo, xray, xray.status.clone());
        }
        let reason = format!(
            "mihomo: {}; xray: {}",
            mihomo.status.reason(),
            xray.status.reason()
        );
        return rebase(mihomo, mihomo, Status::Dead { reason });
    }
    mihomo.clone()
}

/// Copy a combined status onto the node/source of the primary verdict, keeping
/// the measurements of whichever core produced the status.
fn rebase(base: &Checked, source: &Checked, status: Status) -> Checked {
    Checked {
        node: base.node.clone(),
        source: base.source.clone(),
        exit_ip: source.exit_ip,
        country: source.country.clone(),
        latency: source.latency,
        mbps: source.mbps,
        status,
    }
}

/// Give every published node the exact name the brief asks for.
fn name_published(published: &mut [Checked], seed: u64, name: &str) {
    let seed = if seed == 0 {
        crate::util::time_seed()
    } else {
        seed
    };
    let mut ids = Ids::new(seed);
    let mut taken = HashSet::new();
    for checked in published {
        let digits = ids.unique_four_digits(&mut taken);
        checked.node.label = Some(published_name(checked.country.as_deref(), digits, name));
    }
}

/// Recompute the per-source `unique` counts after a filter.
fn recompute_credited(merged: &mut Merged) {
    let mut credited: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for candidate in &merged.candidates {
        *credited.entry(candidate.source.as_str()).or_default() += 1;
    }
    for stat in &mut merged.stats {
        stat.unique = credited.get(stat.name.as_str()).copied().unwrap_or(0);
    }
}

fn count_by_source<F>(checked: &[Checked], keep: F) -> std::collections::HashMap<&str, usize>
where
    F: Fn(&Checked) -> bool,
{
    let mut counts = std::collections::HashMap::new();
    for checked in checked.iter().filter(|checked| keep(checked)) {
        *counts.entry(checked.source.as_str()).or_default() += 1;
    }
    counts
}

fn print_dry_run(
    settings: &Settings,
    fetched: &[FetchedSource],
    merged: &Merged,
    sources_used: usize,
) {
    println!("NetheriteParser (dry run)");
    println!(
        "sources: {} listed, {} answered",
        fetched.len(),
        sources_used
    );
    println!(
        "configs: {} seen, {} parsed, {} merged ({} duplicates, {} cut by --max-nodes)",
        merged.total_cfg,
        merged.total_parsed_cfg,
        merged.candidates.len(),
        merged.duplicates,
        merged.truncated
    );
    let by_protocol = ProtocolCounts::of(
        &merged
            .candidates
            .iter()
            .map(|candidate| candidate.node.clone())
            .collect::<Vec<_>>(),
    );
    println!(
        "protocols: {} vless, {} vmess, {} trojan, {} ss, {} hy2, {} other",
        by_protocol.vless,
        by_protocol.vmess,
        by_protocol.trojan,
        by_protocol.ss,
        by_protocol.hy2,
        by_protocol.other
    );
    println!("nothing was probed and nothing was written");
    if !settings.quiet {
        eprintln!("dry run: use without --dry-run to publish");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use v2keylib::node::{Node, Protocol};

    fn node() -> Node {
        Node::new(Protocol::Vless, "192.0.2.10", 443)
            .with_uuid("00000000-0000-0000-0000-000000000010")
    }

    fn checked(status: Status) -> Checked {
        Checked {
            node: node(),
            source: "test".into(),
            exit_ip: matches!(status, Status::Published).then(|| "198.51.100.7".parse().unwrap()),
            country: matches!(status, Status::Published).then(|| "NL".to_string()),
            latency: None,
            mbps: Some(9.5),
            status,
        }
    }

    #[test]
    fn either_core_publishing_is_enough() {
        let dead = checked(Status::Dead {
            reason: "mihomo said no".into(),
        });
        let alive = checked(Status::Published);
        let combined = combine(&dead, Some(&alive));
        assert!(combined.is_published());
        assert_eq!(combined.country.as_deref(), Some("NL"));
    }

    #[test]
    fn a_blacklist_hit_beats_a_publication_in_the_other_core() {
        let alive = checked(Status::Published);
        let black = checked(Status::Blacklisted {
            address: "203.0.113.9".parse().unwrap(),
            list_entry: "203.0.113.0/24".into(),
        });
        assert!(matches!(
            combine(&alive, Some(&black)).status,
            Status::Blacklisted { .. }
        ));
    }

    #[test]
    fn a_slow_verdict_is_preferred_over_dead() {
        let dead = checked(Status::Dead {
            reason: "no answer".into(),
        });
        let slow = checked(Status::TooSlow {
            mbps: 1.2,
            floor: 5.0,
        });
        assert!(matches!(
            combine(&dead, Some(&slow)).status,
            Status::TooSlow { .. }
        ));
    }

    #[test]
    fn both_dead_keeps_both_reasons() {
        let mihomo = checked(Status::Dead {
            reason: "mihomo: timed out".into(),
        });
        let xray = checked(Status::Dead {
            reason: "xray: refused".into(),
        });
        let combined = combine(&mihomo, Some(&xray));
        let Status::Dead { reason } = combined.status else {
            panic!("expected dead");
        };
        assert!(reason.contains("mihomo: timed out"));
        assert!(reason.contains("xray: refused"));
    }
}
