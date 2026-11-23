//! One source: a name and a URL.
//!
//! Names are for humans and for the usefulness report; they never reach a
//! published node, because a subscription is read for its nodes, not for the
//! repository they were copied from.

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    pub name: String,
    pub url: String,
}

impl SourceEntry {
    /// Parse one line of `sources.txt`.
    ///
    /// Two shapes are accepted: `name URL`, and a bare `URL` whose name is
    /// derived from its host and last path segment. The bare form is what most
    /// people paste, and refusing it would make the file harder to extend than
    /// the tool it feeds.
    pub fn parse(line: &str, line_number: usize) -> Result<Option<Self>> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return Ok(None);
        }
        let (name, url) = match line.split_once(char::is_whitespace) {
            Some((name, rest)) => {
                let rest = rest.trim();
                if rest.is_empty() {
                    return Err(Error::Source {
                        line: line_number,
                        reason: "name without a URL".into(),
                    });
                }
                (Some(name.trim().to_string()), rest.to_string())
            }
            None => (None, line.to_string()),
        };
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(Error::Source {
                line: line_number,
                reason: format!("not an http(s) URL: {url}"),
            });
        }
        let name = name.unwrap_or_else(|| derive_name(&url));
        Ok(Some(Self { name, url }))
    }
}

/// A short, stable name for a bare URL.
///
/// `https://host/path/to/file.txt` becomes `file`; a URL whose path says
/// nothing becomes its host. Collisions are not a correctness problem — the
/// report prints both rows — so no suffix is invented here.
fn derive_name(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let (authority, path) = without_scheme
        .split_once('/')
        .unwrap_or((without_scheme, ""));
    let file = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let stem = file.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(file);
    if stem.is_empty() {
        authority.trim_start_matches("www.").to_string()
    } else {
        stem.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_line() {
        let entry = SourceEntry::parse("jp  https://example.com/list.txt", 1)
            .unwrap()
            .unwrap();
        assert_eq!(entry.name, "jp");
        assert_eq!(entry.url, "https://example.com/list.txt");
    }

    #[test]
    fn bare_url_gets_a_name() {
        let entry = SourceEntry::parse("https://example.com/a/finland.txt", 1)
            .unwrap()
            .unwrap();
        assert_eq!(entry.name, "finland");
    }

    #[test]
    fn comments_and_blanks_are_skipped() {
        assert!(SourceEntry::parse("  # note", 1).unwrap().is_none());
        assert!(SourceEntry::parse("", 2).unwrap().is_none());
    }

    #[test]
    fn a_name_without_a_url_is_an_error() {
        assert!(SourceEntry::parse("only-a-name", 3).is_err());
    }
}
