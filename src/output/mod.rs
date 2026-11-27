//! Writing the published files.

pub mod dead;
pub mod headers;
pub mod links;
pub mod meta;
pub mod mihomo;
pub mod xray;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use v2keylib::node::{Node, Protocol};

use crate::error::Result;
use crate::models::{Checked, Outcome, SourceStat};
use crate::settings::Settings;
use crate::sources;
use crate::util::write_text;

#[derive(Clone, Debug)]
pub struct Files {
    pub default: PathBuf,
    pub mihomo: PathBuf,
    pub meta: PathBuf,
    pub dead: PathBuf,
    pub report: PathBuf,
    pub xray: PathBuf,
}

impl Files {
    pub fn paths(settings: &Settings) -> Self {
        Self {
            default: settings.out_dir.join("default"),
            mihomo: settings.out_dir.join("mihomo"),
            meta: settings.out_dir.join("meta.json"),
            dead: settings.dead_path(),
            report: settings.report_path(),
            xray: settings.out_dir.join("xray.json"),
        }
    }
}

/// Write every output file.
pub fn write_all(
    settings: &Settings,
    outcome: &Outcome,
    checked: &[Checked],
    stats: &[SourceStat],
) -> Result<Files> {
    let files = Files::paths(settings);

    // Full txt sub
    let (default_body, _skipped) = links::render(&outcome.xray_nodes, headers::full_type())?;
    write_text(&files.default, &default_body)?;

    // Mihomo yaml
    let mihomo_body = mihomo::render(&outcome.mihomo_nodes)?;
    write_text(&files.mihomo, &mihomo_body)?;

    write_text(&files.meta, &meta::render(outcome))?;
    write_text(&files.dead, &dead::render(checked))?;

    let report = sources::Report { stats }.render();
    write_text(&files.report, &report)?;

    // Build country lookup from checked (published xray nodes)
    let mut country_by_key: HashMap<String, String> = HashMap::new();
    for c in checked.iter().filter(|c| c.is_published()) {
        if let Some(cc) = c.country.as_ref() {
            country_by_key.insert(c.node.strict_key(), cc.clone());
        }
    }

    let xray_nodes = &outcome.xray_nodes;
    write_splits(&settings.out_dir, xray_nodes, &country_by_key)?;
    let xray_json = xray::render(xray_nodes, &country_by_key);
    write_text(&files.xray, &xray_json)?;

    Ok(files)
}

fn write_splits(out_dir: &Path, nodes: &[Node], country_by_key: &HashMap<String, String>) -> Result<()> {
    // Protocols
    let mut by_proto: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    for n in nodes {
        let p = proto_name(n.protocol).to_string();
        by_proto.entry(p).or_default().push(n.clone());
    }
    for (p, group) in &by_proto {
        let (body, _) = links::render(group, &headers::protocol_type(p))?;
        write_text(&out_dir.join("protocols").join(format!("{}.txt", p.to_lowercase())), &body)?;
    }

    // Transports
    let mut by_transport: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    for n in nodes {
        let t = n.transport.kind.as_str().to_string();
        by_transport.entry(t).or_default().push(n.clone());
    }
    for (t, group) in &by_transport {
        let (body, _) = links::render(group, &headers::transport_type(t))?;
        write_text(&out_dir.join("transports").join(format!("{}.txt", sanitize_name(t))), &body)?;
    }

    // Countries
    let mut by_country: BTreeMap<String, (String, Vec<Node>)> = BTreeMap::new();
    for n in nodes {
        let cc = country_by_key
            .get(&n.strict_key())
            .cloned()
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let name = crate::geo::country_name(Some(&cc));
        let entry = by_country.entry(cc.clone()).or_insert_with(|| (name, Vec::new()));
        entry.1.push(n.clone());
    }
    for (cc, (name, group)) in &by_country {
        let display = if cc == "UNKNOWN" { "Неизвестно".to_string() } else { name.clone() };
        let type_friendly = headers::country_type(&display);
        let (body, _) = links::render(group, &type_friendly)?;
        let file = if cc == "UNKNOWN" { "UNKNOWN.txt".to_string() } else { format!("{cc}.txt") };
        write_text(&out_dir.join("countries").join(file), &body)?;
    }

    Ok(())
}

fn proto_name(p: Protocol) -> &'static str {
    match p {
        Protocol::Vless => "vless",
        Protocol::Vmess => "vmess",
        Protocol::Trojan => "trojan",
        Protocol::Shadowsocks => "ss",
        Protocol::Hysteria2 => "hy2",
        _ => "other",
    }
}

fn sanitize_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if out.is_empty() { out.push_str("unknown"); }
    out
}


pub fn directory(settings: &Settings) -> &Path {
    &settings.out_dir
}
