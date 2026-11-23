//! Read and match the address blacklist.
//!
//! The list is about addresses, not names: a node whose *entry* is a blacklisted
//! address must never be probed, and a node whose *exit* turns out to be one must
//! never be published. Both checks are here so the pipeline cannot apply one and
//! forget the other.

use std::net::IpAddr;
use std::path::Path;

use v2keylib::filter::ip::IpMatcher;

use crate::error::{Error, Result};
use crate::util::read_text;

#[derive(Clone, Debug)]
pub struct Entry {
    /// The line as written, for the report: `203.0.113.0/24` says more than a
    /// parsed network does.
    pub raw: String,
    pub matcher: IpMatcher,
}

#[derive(Clone, Debug, Default)]
pub struct Blacklist {
    entries: Vec<Entry>,
}

impl Blacklist {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Load a file, treating absence as an empty list.
    ///
    /// A blacklist is optional: a run without one is useful, and a missing file
    /// next to a cloned repository should not stop it. Only an unreadable or
    /// malformed file that *exists* is an error.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::empty());
        }
        let text = read_text(path)?;
        Self::parse(&text).map_err(|e| match e {
            Error::Source { line, reason } => {
                Error::Config(format!("{}:{line}: {reason}", path.display()))
            }
            other => other,
        })
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut entries = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            // A trailing comment is common in hand-edited lists.
            let value = line.split_whitespace().next().unwrap_or(line);
            let matcher = IpMatcher::parse(value).map_err(|e| Error::Source {
                line: index + 1,
                reason: format!("{value}: {e}"),
            })?;
            entries.push(Entry {
                raw: value.to_string(),
                matcher,
            });
        }
        Ok(Self { entries })
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The first entry that covers `ip`, if any.
    pub fn hit(&self, ip: IpAddr) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.matcher.matches(&ip))
    }

    /// The first entry that covers any of `ips`.
    pub fn hit_any(&self, ips: impl IntoIterator<Item = IpAddr>) -> Option<(IpAddr, &Entry)> {
        for ip in ips {
            if let Some(entry) = self.hit(ip) {
                return Some((ip, entry));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_and_cidr() {
        let list = Blacklist::parse("# note\n203.0.113.9\n198.51.100.0/24\n").unwrap();
        assert!(list.hit("203.0.113.9".parse().unwrap()).is_some());
        assert!(list.hit("198.51.100.44".parse().unwrap()).is_some());
        assert!(list.hit("198.51.101.44".parse().unwrap()).is_none());
    }

    #[test]
    fn comments_do_not_become_matches() {
        let list = Blacklist::parse("# 10.0.0.0/8\n\n").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn a_bad_line_names_its_number() {
        let error = Blacklist::parse("1.2.3.4\nnot-an-ip\n").unwrap_err();
        assert!(error.to_string().contains("2"));
    }
}
