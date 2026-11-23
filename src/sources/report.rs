//! The per-source usefulness table.
//!
//! A source is useful when its nodes survive the whole run, not when it returns
//! a lot of text. The table therefore ranks by `passed`, then `alive`, and
//! prints the yield so a list of four thousand addresses that produces no
//! working node does not look productive next to a list of twenty that produces
//! three.

use crate::models::SourceStat;

pub struct Report<'a> {
    pub stats: &'a [SourceStat],
}

impl<'a> Report<'a> {
    /// The full table, ready to write to a file.
    pub fn render(&self) -> String {
        let mut lines = Vec::new();
        lines.push("NetheriteParser source report".to_string());
        lines.push("============================".to_string());
        lines.push(String::new());
        lines.push(
            "passed = published in at least one subscription after both probes; unique = credited after dedup"
                .to_string(),
        );
        lines.push(String::new());
        lines.push(format!(
            "{:<18} {:>7} {:>7} {:>7} {:>7} {:>7}  {}",
            "source", "raw", "parsed", "unique", "alive", "passed", "note"
        ));
        lines.push("-".repeat(78));

        let mut ranked: Vec<&SourceStat> = self.stats.iter().collect();
        ranked.sort_by(|a, b| {
            b.passed
                .cmp(&a.passed)
                .then(b.alive.cmp(&a.alive))
                .then(b.unique.cmp(&a.unique))
                .then(a.name.cmp(&b.name))
        });

        for stat in &ranked {
            let note = match &stat.error {
                Some(error) => format!("error: {error}"),
                None if stat.parsed == 0 => "empty or not a subscription".to_string(),
                None if stat.unique == 0 => "duplicate of an earlier source".to_string(),
                None => format!("{:.1}% yield", stat.yield_percent()),
            };
            lines.push(format!(
                "{:<18} {:>7} {:>7} {:>7} {:>7} {:>7}  {}",
                truncate(&stat.name, 18),
                stat.raw_links,
                stat.parsed,
                stat.unique,
                stat.alive,
                stat.passed,
                note
            ));
        }

        let useful: Vec<&SourceStat> = ranked.iter().copied().filter(|s| s.passed > 0).collect();
        lines.push(String::new());
        if useful.is_empty() {
            lines.push("No source produced a published node in this run.".to_string());
        } else {
            lines.push("Most useful sources (by published nodes):".to_string());
            for (rank, stat) in useful.iter().take(10).enumerate() {
                lines.push(format!(
                    "  {}. {} — {} passed ({} answered, {:.1}% yield)",
                    rank + 1,
                    stat.name,
                    stat.passed,
                    stat.alive,
                    stat.yield_percent()
                ));
            }
        }
        lines.join("\n") + "\n"
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let cut: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(name: &str, parsed: usize, unique: usize, alive: usize, passed: usize) -> SourceStat {
        SourceStat {
            name: name.into(),
            parsed,
            unique,
            alive,
            passed,
            ..Default::default()
        }
    }

    #[test]
    fn ranks_published_first() {
        let stats = [
            stat("big-but-dead", 1000, 900, 4, 0),
            stat("small-and-good", 10, 10, 6, 5),
        ];
        let text = Report { stats: &stats }.render();
        let good = text.find("small-and-good").unwrap();
        let dead = text.find("big-but-dead").unwrap();
        assert!(good < dead, "{text}");
    }

    #[test]
    fn says_so_when_nothing_passed() {
        let stats = [stat("x", 4, 4, 0, 0)];
        assert!(Report { stats: &stats }
            .render()
            .contains("No source produced"));
    }

    #[test]
    fn text_file_ends_with_a_newline() {
        let stats = [stat("x", 4, 4, 0, 0)];
        assert!(Report { stats: &stats }.render().ends_with('\n'));
    }
}
