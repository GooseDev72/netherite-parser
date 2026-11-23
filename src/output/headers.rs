//! Announce/header helpers for subscriptions.

/// Build the announce text for a subscription.
pub fn announce(nodes: usize, type_friendly: &str) -> String {
    format!(
        "🪨 Netherite Parser | {nodes} нод | {type_friendly}\n🔌 автор @goosedev72_vpnsubs / @GooseDev72"
    )
}

/// Render announce as comment lines for pure txt subs.
pub fn announce_comments(nodes: usize, type_friendly: &str) -> String {
    let text = announce(nodes, type_friendly);
    let mut out = String::new();
    for line in text.lines() {
        out.push_str("# ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Type friendly for full subscription.
pub fn full_type() -> &'static str { "полная подписка" }
pub fn protocol_type(proto: &str) -> String { format!("фильтр по {}", proto.to_uppercase()) }
pub fn country_type(country: &str) -> String { format!("фильтр по {country}") }
pub fn transport_type(t: &str) -> String { format!("фильтр по {t}") }
