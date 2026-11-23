//! The final summary on stdout.
//!
//! Everything a person needs after a run that may have taken an hour: what came
//! in, what was thrown away and why, what is in the files, and which sources
//! earned their place. The same information is on disk in `meta.json` and
//! `source_report.txt`; this is the version that does not require opening
//! anything.

use crate::models::{Outcome, ProtocolCounts, SourceStat};
use crate::output::Files;
use crate::util::{human_duration, human_mbps};

pub struct Summary<'a> {
    pub outcome: &'a Outcome,
    pub stats: &'a [SourceStat],
    pub files: &'a Files,
    pub sources_total: usize,
    pub sources_used: usize,
    pub duplicates: usize,
    pub truncated: usize,
}

impl Summary<'_> {
    pub fn render(&self) -> String {
        let outcome = self.outcome;
        let mut lines = Vec::new();
        lines.push("NetheriteParser".to_string());
        lines.push(format!(
            "sources: {} listed, {} answered",
            self.sources_total, self.sources_used
        ));
        lines.push(format!(
            "configs: {} seen, {} parsed, {} merged ({} duplicates, {} cut by --max-nodes)",
            outcome.total_cfg,
            outcome.total_parsed_cfg,
            outcome.total_merged_cfg,
            self.duplicates,
            self.truncated
        ));
        let baseline = outcome
            .baseline_mbps
            .map(|mbps| format!("{} Mbit/s", human_mbps(mbps)))
            .unwrap_or_else(|| "not measured".into());
        if outcome.threshold_mbps <= 0.0 {
            lines.push(format!("line: {baseline}; speed filter disabled"));
        } else {
            lines.push(format!(
                "line: {baseline}; published nodes must reach {} Mbit/s",
                human_mbps(outcome.threshold_mbps)
            ));
        }
        lines.push(format!(
            "result: {} of {} published ({} Xray, {} mihomo, {} in both)",
            outcome.published_total,
            outcome.total_merged_cfg,
            outcome.xray_nodes.len(),
            outcome.mihomo_nodes.len(),
            outcome.published_both
        ));
        lines.push(format!(
            "rejected: {} dead, {} too slow, {} blacklisted",
            outcome
                .total_dead_cfg
                .saturating_sub(outcome.total_too_slow_cfg)
                .saturating_sub(outcome.total_blacklisted_cfg),
            outcome.total_too_slow_cfg,
            outcome.total_blacklisted_cfg
        ));
        lines.push(protocol_line("xray", &outcome.xray_counts));
        lines.push(protocol_line("mihomo", &outcome.mihomo_counts));
        lines.push(format!("elapsed: {}", human_duration(outcome.elapsed)));

        let mut useful: Vec<&SourceStat> = self.stats.iter().filter(|s| s.passed > 0).collect();
        useful.sort_by(|a, b| b.passed.cmp(&a.passed).then(a.name.cmp(&b.name)));
        lines.push(String::new());
        if useful.is_empty() {
            lines.push("most useful sources: none produced a published node".to_string());
        } else {
            lines.push("most useful sources:".to_string());
            for stat in useful.iter().take(5) {
                lines.push(format!(
                    "  {:<18} {} passed of {} ({} answered)",
                    stat.name, stat.passed, stat.unique, stat.alive
                ));
            }
        }

        lines.push(String::new());
        lines.push(format!("wrote {}", self.files.default.display()));
        lines.push(format!("wrote {}", self.files.mihomo.display()));
        lines.push(format!("wrote {}", self.files.meta.display()));
        lines.push(format!("wrote {}", self.files.dead.display()));
        lines.push(format!("wrote {}", self.files.report.display()));
        lines.join("\n")
    }
}

/// One protocol line per core, so the two published sets can be compared
/// without opening `meta.json`.
fn protocol_line(label: &str, counts: &ProtocolCounts) -> String {
    let mut line = format!(
        "protocols {label}: {} vless, {} vmess, {} trojan, {} ss, {} hy2",
        counts.vless, counts.vmess, counts.trojan, counts.ss, counts.hy2
    );
    if counts.other > 0 {
        line.push_str(&format!(", {} other", counts.other));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProtocolCounts;
    use std::path::PathBuf;

    fn outcome() -> Outcome {
        Outcome {
            mihomo_version: "Mihomo Meta v1.19.26".into(),
            xray_version: "Xray 26.3.27".into(),
            total_cfg: 100,
            total_parsed_cfg: 90,
            total_merged_cfg: 70,
            total_dead_cfg: 66,
            total_blacklisted_cfg: 1,
            total_too_slow_cfg: 3,
            published_total: 4,
            published_both: 2,
            mihomo_nodes: Vec::new(),
            xray_nodes: Vec::new(),
            mihomo_counts: ProtocolCounts::default(),
            xray_counts: ProtocolCounts::default(),
            baseline_mbps: Some(46.3),
            threshold_mbps: 5.0,
            started_at: std::time::SystemTime::now(),
            elapsed: std::time::Duration::from_secs(12),
        }
    }

    #[test]
    fn summary_states_the_floor_and_the_losses() {
        let outcome = outcome();
        let files = Files {
            default: PathBuf::from("sub/default"),
            mihomo: PathBuf::from("sub/mihomo"),
            meta: PathBuf::from("sub/meta.json"),
            dead: PathBuf::from("sub/dead.txt"),
            report: PathBuf::from("sub/source_report.txt"),
            balancers: PathBuf::from("sub/balancers.json"),
        };
        let text = Summary {
            outcome: &outcome,
            stats: &[],
            files: &files,
            sources_total: 1,
            sources_used: 1,
            duplicates: 20,
            truncated: 0,
        }
        .render();
        assert!(text.contains("5.0 Mbit/s"));
        assert!(text.contains("3 too slow"));
        assert!(text.contains("rejected: 62 dead, 3 too slow, 1 blacklisted"));
        assert!(text.contains("sub/mihomo"));
    }
}
