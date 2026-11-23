//! `meta.json`: what was run and what came out.
//!
//! The counts are deliberately about different stages, because that is the only
//! way the file answers "where did the nodes go": seen, parsed, merged, dead,
//! then the two published sets and their protocol split.

use serde_json::json;

use crate::models::Outcome;

pub fn render(outcome: &Outcome) -> String {
    // Pretty-printed: this file is read by tools and by people asking why a run
    // produced so little, and a one-line object answers neither.
    let value = json!({
        "mihomo_ver": outcome.mihomo_version,
        "xray_ver": outcome.xray_version,
        "total_cfg": outcome.total_cfg,
        "total_parsed_cfg": outcome.total_parsed_cfg,
        "total_merged_cfg": outcome.total_merged_cfg,
        "total_dead_cfg": outcome.total_dead_cfg,
        "total_xray_cfg": outcome.total_xray_cfg(),
        "total_mihomo_cfg": outcome.total_mihomo_cfg(),
        "total_vless_cfg": outcome.mihomo_counts.vless,
        "total_ss_cfg": outcome.mihomo_counts.ss,
        "total_trojan_cfg": outcome.mihomo_counts.trojan,
        "total_vmess_cfg": outcome.mihomo_counts.vmess,
        "total_hy2_cfg": outcome.mihomo_counts.hy2,
        "mihomo_compatible": outcome.mihomo_counts.as_json(),
        "xray_compatible": outcome.xray_counts.as_json(),
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()) + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProtocolCounts;

    #[test]
    fn every_required_field_is_present() {
        let outcome = Outcome {
            mihomo_version: "Mihomo Meta v1.19.26".into(),
            xray_version: "Xray 26.3.27".into(),
            total_cfg: 100,
            total_parsed_cfg: 90,
            total_merged_cfg: 70,
            total_dead_cfg: 65,
            total_blacklisted_cfg: 2,
            total_too_slow_cfg: 3,
            published_total: 68,
            published_both: 60,
            mihomo_nodes: Vec::new(),
            xray_nodes: Vec::new(),
            mihomo_counts: ProtocolCounts {
                vless: 4,
                ss: 1,
                ..Default::default()
            },
            xray_counts: ProtocolCounts {
                vless: 5,
                ss: 2,
                ..Default::default()
            },
            baseline_mbps: Some(46.3),
            threshold_mbps: 5.0,
            started_at: std::time::SystemTime::now(),
            elapsed: std::time::Duration::ZERO,
        };
        let text = render(&outcome);
        for field in [
            "mihomo_ver",
            "xray_ver",
            "total_cfg",
            "total_parsed_cfg",
            "total_merged_cfg",
            "total_dead_cfg",
            "total_xray_cfg",
            "total_mihomo_cfg",
            "total_vless_cfg",
            "total_ss_cfg",
            "total_trojan_cfg",
            "total_vmess_cfg",
            "total_hy2_cfg",
            "mihomo_compatible",
            "xray_compatible",
        ] {
            assert!(text.contains(&format!("\"{field}\"")), "{field} missing");
        }
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["total_vless_cfg"], 4);
        assert_eq!(value["mihomo_compatible"]["vless"], 4);
        assert_eq!(value["xray_compatible"]["vless"], 5);
        assert_eq!(value["xray_compatible"]["ss"], 2);
        assert_eq!(value["total_ss_cfg"], 1);
    }
}
