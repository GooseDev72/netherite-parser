//! Read `sources.txt`.
//!
//! The file is intentionally plain: a name and a URL per line, comments with
//! `#`. It is also the place where a forgotten source is visible, which a
//! binary config would hide.

use std::path::Path;

use crate::error::Result;
use crate::util::read_text;

use super::entry::SourceEntry;

#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub entries: Vec<SourceEntry>,
    /// URLs that appeared more than once and were folded into the first entry.
    pub duplicates: usize,
}

impl Registry {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Load and validate a source list.
    ///
    /// A duplicate URL is not an error: an export is often listed under two
    /// names while it moves between repositories. It is folded so the same file
    /// is not fetched twice, and counted so the report can say so.
    ///
    /// File order is preserved. It is the only priority control the user has:
    /// `--max-sources` takes the first N, and the merge interleaves in this
    /// order, so the sources at the top of the file are the ones a limited run
    /// actually measures.
    pub fn load(path: &Path) -> Result<Self> {
        let text = read_text(path)?;
        let mut registry = Self::default();
        let mut seen = std::collections::HashSet::new();
        for (index, line) in text.lines().enumerate() {
            let Some(entry) = SourceEntry::parse(line, index + 1)? else {
                continue;
            };
            if seen.insert(entry.url.clone()) {
                registry.entries.push(entry);
            } else {
                registry.duplicates += 1;
            }
        }
        Ok(registry)
    }

    /// The first `limit` entries, or all when `limit` is zero.
    pub fn take_all_or(&self, limit: usize) -> Vec<SourceEntry> {
        if limit == 0 {
            self.entries.clone()
        } else {
            self.entries.iter().take(limit).cloned().collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicates_are_folded_and_counted() {
        let dir = std::env::temp_dir().join(format!("np-reg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sources.txt");
        std::fs::write(&path, "a https://example.com/x\nb https://example.com/x\n").unwrap();
        let registry = Registry::load(&path).unwrap();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.duplicates, 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
