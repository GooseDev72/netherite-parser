//! Small helpers shared by the modules.
//!
//! Nothing here knows about proxies. It is the arithmetic that would otherwise
//! be copied into three files: a readable speed, a readable duration, file
//! reads that name the path, and the four-digit identifier that makes a node
//! name unique without making it meaningful.

use std::path::Path;

use crate::error::{Error, Result};

/// A line like `1.2 s` or `340 ms`.
pub fn human_duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", d.as_secs_f64())
    }
}

/// A line like `42.7` — one decimal is all a speed number deserves.
pub fn human_mbps(mbps: f64) -> String {
    format!("{mbps:.1}")
}

/// Read a text file, naming it in the error.
///
/// `std::fs::read_to_string` says "No such file or directory" and leaves the
/// caller to guess which file; a run has a dozen inputs.
pub fn read_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| Error::io(path, e))
}

/// Write a text file, creating its parent directory first.
///
/// Every output in this program is a file in one directory, and a fresh clone
/// does not have that directory. Creating it here means no caller has to
/// remember, and a partially created output tree cannot happen because the
/// failure happens at the first write.
pub fn write_text(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::write(path, body).map_err(|e| Error::io(path, e))
}

/// A tiny deterministic generator for the four-digit part of a name.
///
/// `rand` would work, but the only requirement is "four digits, not the same as
/// the last one". A seeded xorshift is enough, reproducible with `--seed`, and
/// has no dependency to keep in step with the toolchain.
#[derive(Clone, Debug)]
pub struct Ids {
    state: u64,
}

impl Ids {
    pub fn new(seed: u64) -> Self {
        // Any non-zero state is valid; zero is the one state xorshift cannot
        // leave.
        Self { state: seed.max(1) }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// A number in `1000..=9999`.
    ///
    /// Four digits, never zero-padded: the name should read as a number, not as
    /// a code.
    pub fn four_digits(&mut self) -> u16 {
        1000 + (self.next() % 9000) as u16
    }

    /// A number in `1000..=9999` that is not already taken.
    ///
    /// Duplicates are harmless to a client but confusing to a person: two rows
    /// with the same suffix look like two sources of the same node. On a
    /// subscription of a few hundred the space is large enough that retrying is
    /// cheaper than a different scheme.
    pub fn unique_four_digits(&mut self, taken: &mut std::collections::HashSet<u16>) -> u16 {
        loop {
            let id = self.four_digits();
            if taken.insert(id) {
                return id;
            }
        }
    }
}

/// A seed that is stable for one run and different between runs.
pub fn time_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15);
    nanos ^ (std::process::id() as u64).wrapping_mul(0x9E3779B97F4A7C15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_digits_stay_in_range() {
        let mut ids = Ids::new(7);
        for _ in 0..10_000 {
            let id = ids.four_digits();
            assert!((1000..=9999).contains(&id), "{id}");
        }
    }

    #[test]
    fn unique_ids_do_not_repeat() {
        let mut ids = Ids::new(11);
        let mut taken = std::collections::HashSet::new();
        for _ in 0..500 {
            let id = ids.unique_four_digits(&mut taken);
            assert!((1000..=9999).contains(&id));
        }
        assert_eq!(taken.len(), 500);
    }
}
