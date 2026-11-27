use std::collections::HashSet;

use crate::models::{Checked, Status};
use crate::collect::Merged;
use crate::geo::published_name;
use crate::settings::Settings;
use crate::sources::FetchedSource;
use crate::util::Ids;
use crate::models::ProtocolCounts;

/// Fold the two core verdicts into one ledger for `dead.txt` and the report.
///
/// A node is published when either core published it. A blacklist hit in either
/// core wins over a publication in the other: the brief says a forbidden exit
/// is never published, and that must not depend on which client noticed it.
/// Otherwise the node is too slow when either core measured it below the floor,
/// and dead when neither core got an answer.
pub fn merge_checked(mihomo: &[Checked], xray: &[Checked]) -> Vec<Checked> {
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
pub fn combine(mihomo: &Checked, xray: Option<&Checked>) -> Checked {
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
pub fn rebase(base: &Checked, source: &Checked, status: Status) -> Checked {
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
pub fn name_published(published: &mut [Checked], seed: u64, name: &str) {
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
pub fn recompute_credited(merged: &mut Merged) {
    let mut credited: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for candidate in &merged.candidates {
        *credited.entry(candidate.source.as_str()).or_default() += 1;
    }
    for stat in &mut merged.stats {
        stat.unique = credited.get(stat.name.as_str()).copied().unwrap_or(0);
    }
}

pub fn count_by_source<F>(checked: &[Checked], keep: F) -> std::collections::HashMap<&str, usize>
where
    F: Fn(&Checked) -> bool,
{
    let mut counts = std::collections::HashMap::new();
    for checked in checked.iter().filter(|checked| keep(checked)) {
        *counts.entry(checked.source.as_str()).or_default() += 1;
    }
    counts
}

pub fn print_dry_run(
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
