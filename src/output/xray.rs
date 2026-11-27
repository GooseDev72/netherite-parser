//! Xray JSON profiles (Aeryx/Happ-style array), replaces balancers.json.

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};
use v2keylib::node::Node;

fn profile_template(remarks: &str) -> Value {
    json!({
        "remarks": remarks,
        "log": {"loglevel": "warning"},
        "dns": {
            "hosts": {
                "dns.google": ["8.8.8.8","8.8.4.4","2001:4860:4860::8888","2001:4860:4860::8844"],
                "dns.alidns.com": ["223.5.5.5","223.6.6.6","2400:3200::1","2400:3200:baba::1"],
                "one.one.one.one": ["1.1.1.1","1.0.0.1","2606:4700:4700::1111","2606:4700:4700::1001"],
                "dns.cloudflare.com": ["1.1.1.1","1.0.0.1","2606:4700:4700::1111","2606:4700:4700::1001"]
            },
            "servers": [
                "https://dns.google/dns-query",
                {"address": "https://dns.google/dns-query", "domains": ["domain:googleapis.cn","domain:gstatic.com"], "skipFallback": true},
                {"address": "https://dns.alidns.com/dns-query", "domains": ["geosite:cn"], "skipFallback": true, "expectIPs": ["geoip:cn"]},
                {"address": "https://dns.alidns.com/dns-query", "skipFallback": true, "expectIPs": ["geoip:cn"]}
            ]
        },
        "inbounds": [
            {"tag":"socks","port":10808,"listen":"127.0.0.1","protocol":"socks","sniffing":{"enabled":true,"destOverride":["http","tls"],"routeOnly":false},"settings":{"auth":"noauth","udp":true,"allowTransparent":false}},
            {"tag":"http","port":10809,"listen":"127.0.0.1","protocol":"http","sniffing":{"enabled":true,"destOverride":["http","tls"],"routeOnly":false},"settings":{"auth":"noauth","udp":true,"allowTransparent":false}},
            {"listen":"127.0.0.1","port":10853,"protocol":"dokodemo-door","settings":{"address":"1.1.1.1","network":"tcp,udp","port":53},"tag":"dns-in"}
        ],
        "outbounds": [
            {"tag":"direct","protocol":"freedom","settings":{"domainStrategy":"UseIP"}},
            {"tag":"block","protocol":"blackhole","settings":{"response":{"type":"http"}}},
            {"tag":"dns-out","protocol":"dns"}
        ],
        "routing": {
            "domainStrategy":"AsIs",
            "rules": [
                {"type":"field","inboundTag":["dns-in"],"outboundTag":"dns-out"},
                {"type":"field","ip":["geoip:private"],"outboundTag":"direct"},
                {"type":"field","balancerTag":"balancer"}
            ],
            "balancers": [
                {"tag":"balancer","selector":["proxy"],"strategy":{"type":"leastPing"}}
            ]
        },
        "burstObservatory": {
            "subjectSelector":["proxy"],
            "pingConfig":{"connectivity":"https://www.gstatic.com/generate_204","destination":"https://www.google.com/generate_204","interval":"60s","timeout":"3s","sampling":2}
        },
        "stats": {},
        "policy": {
            "levels": {"8": {"connIdle":300,"downlinkOnly":1,"handshake":4,"uplinkOnly":1}},
            "system": {"showJSONError":true}
        }
    })
}

fn build_outbounds(nodes: &[Node]) -> Vec<Value> {
    if nodes.is_empty() { return Vec::new(); }
    let tags: Vec<String> = (1..=nodes.len()).map(|i| format!("proxy-{i}")).collect();
    // Use v2keylib to render Xray config and extract outbounds
    match v2keylib::config::xray::export_config_tagged(nodes, &tags, None) {
        Ok(cfg) => {
            let mut out = Vec::new();
            if let Some(arr) = cfg.get("outbounds").and_then(|v| v.as_array()) {
                for ob in arr {
                    let tag = ob.get("tag").and_then(|v| v.as_str()).unwrap_or("");
                    let proto = ob.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
                    if tag == "direct" || proto == "freedom" || proto == "blackhole" || proto == "dns" { continue; }
                    // ensure sockopt
                    let mut ob = ob.clone();
                    if let Some(stream) = ob.get_mut("streamSettings").and_then(|v| v.as_object_mut()) {
                        let sockopt = stream.entry("sockopt").or_insert_with(|| json!({}));
                        if let Some(so) = sockopt.as_object_mut() {
                            so.entry("domainStrategy").or_insert(json!("UseIP"));
                            so.entry("tcpKeepAliveIdle").or_insert(json!(100));
                            so.entry("tcpNoDelay").or_insert(json!(true));
                        }
                    }
                    out.push(ob);
                }
            }
            out
        }
        Err(_) => {
            // fallback: per-node
            let mut out = Vec::new();
            for (i, n) in nodes.iter().enumerate() {
                let tag = format!("proxy-{}", i+1);
                if let Ok(cfg) = v2keylib::config::xray::export_config_tagged(&[n.clone()], &[tag.clone()], None) {
                    if let Some(arr) = cfg.get("outbounds").and_then(|v| v.as_array()) {
                        for ob in arr {
                            let proto = ob.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
                            if proto == "freedom" || proto == "blackhole" || proto == "dns" { continue; }
                            out.push(ob.clone());
                        }
                    }
                }
            }
            out
        }
    }
}

fn build_profile(remarks: &str, nodes: &[Node]) -> Option<Value> {
    let proxy_outbounds = build_outbounds(nodes);
    if proxy_outbounds.is_empty() { return None; }
    let mut profile = profile_template(remarks);
    let base_outbounds = profile.get("outbounds").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut all = proxy_outbounds;
    all.extend(base_outbounds);
    profile["outbounds"] = Value::Array(all);
    Some(profile)
}

pub fn render(nodes: &[Node], country_by_key: &HashMap<String, String>) -> String {
    let mut by_country: BTreeMap<String, (String, Vec<Node>)> = BTreeMap::new();
    for n in nodes {
        let cc = country_by_key.get(&n.strict_key()).cloned().unwrap_or_else(|| "UNKNOWN".into());
        let label = country_group_label(&cc);
        by_country.entry(cc).or_insert_with(|| (label, Vec::new())).1.push(n.clone());
    }

    let mut profiles: Vec<Value> = Vec::new();

    // Auto profile with all nodes
    if let Some(auto) = build_profile("🇪🇺 Автовыбор", nodes) {
        profiles.push(auto);
    }

    for (_cc, (label, group)) in &by_country {
        if group.is_empty() { continue; }
        if let Some(p) = build_profile(label, group) {
            profiles.push(p);
        }
    }

    serde_json::to_string_pretty(&profiles).unwrap_or_else(|_| "[]".into()) + "\n"
}

fn country_group_label(cc: &str) -> String {
    if cc == "UNKNOWN" { return "🇸🇴 Неизвестно".to_string(); }
    let flag = crate::geo::flags::flag(Some(cc));
    let name = crate::geo::country_name(Some(cc));
    format!("{flag} {name}")
}
