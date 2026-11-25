//! Live progress for a long run.
//!
//! Two things happen on stderr while a run is working: sources are downloaded,
//! then every candidate is measured. Both can take minutes, and a line that
//! never moves is indistinguishable from a hung process. So the terminal gets
//! one redrawn line: a spinner, a bar, counters and the last node. Logs (a pipe
//! or a file) get a line every five percent instead of one per node, because a
//! thousand identical lines are not more informative than twenty.
//!
//! The painter is a separate thread. The check tasks are async and a node that
//! never answers sits there for the whole timeout; the spinner has to keep
//! turning during exactly that wait, not only when a result arrives.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::models::{Checked, Status};
use crate::util::{human_duration, human_mbps};

const BAR_WIDTH: usize = 24;
const SPIN: [&str; 4] = ["|", "/", "-", "\\"];
/// One lock for stderr: the painter thread and the main thread both write, and
/// without it a note lands in the middle of a redrawn bar.
static OUT: Mutex<()> = Mutex::new(());

/// The width of the terminal, or a width that is wide enough for the full line.
fn terminal_width() -> usize {
    if let Ok(text) = std::env::var("COLUMNS") {
        if let Ok(value) = text.trim().parse::<usize>() {
            if value > 20 {
                return value;
            }
        }
    }
    std::process::Command::new("tput")
        .arg("cols")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| text.trim().parse::<usize>().ok())
        .filter(|width| *width > 20)
        .unwrap_or(100)
}

fn percent_of(done: usize, total: usize) -> usize {
    if total == 0 {
        return 100;
    }
    done.min(total) * 100 / total
}

/// Cut at a character boundary; `&text[..n]` would panic on a flag emoji.
fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars().take(width).collect()
}

fn bar(done: usize, total: usize, width: usize) -> String {
    let filled = width * done.min(total) / total.max(1);
    format!(
        "{}{}",
        "#".repeat(filled.min(width)),
        ".".repeat(width - filled.min(width))
    )
}

/// `5m12s` or `38s`; an estimate with one unit is read, not decoded.
fn eta(done: usize, total: usize, elapsed: Duration) -> String {
    if done == 0 || done >= total {
        return String::new();
    }
    let each = elapsed.as_secs_f64() / done as f64;
    let left = (each * (total - done) as f64).round() as u64;
    if left < 60 {
        format!("  eta {left}s")
    } else {
        format!("  eta {}m{:02}s", left / 60, left % 60)
    }
}

struct State {
    total: AtomicUsize,
    done: AtomicUsize,
    published: AtomicUsize,
    slow: AtomicUsize,
    dead: AtomicUsize,
    timeout: AtomicUsize,
    error: AtomicUsize,
    blacklisted: AtomicUsize,
    frame: AtomicUsize,
    began: Instant,
    tail: Mutex<String>,
    stopped: AtomicBool,
}

impl State {
    fn stat_line(&self) -> String {
        let done = self.done.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        let published = self.published.load(Ordering::Relaxed);
        let timeout = self.timeout.load(Ordering::Relaxed);
        let error = self.error.load(Ordering::Relaxed);
        let pending = total.saturating_sub(done);
        format!("stat: pass {published} timeout {timeout} err {error} pending {pending}")
    }

    fn line(&self, width: usize) -> String {
        let done = self.done.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        let published = self.published.load(Ordering::Relaxed);
        let slow = self.slow.load(Ordering::Relaxed);
        let dead = self.dead.load(Ordering::Relaxed);
        let blacklisted = self.blacklisted.load(Ordering::Relaxed);
        let spin = SPIN[self.frame.load(Ordering::Relaxed) % SPIN.len()];
        let percent = percent_of(done, total);
        let remaining = eta(done, total, self.began.elapsed());
        let counters = format!("  ok {published}  slow {slow}  dead {dead}  bl {blacklisted}");

        // Start with everything and give up detail in the order that hurts
        // least: bar width, counters, ETA. The last node stays until the line
        // is genuinely too narrow.
        let mut width_of_bar = BAR_WIDTH;
        let base = loop {
            let base = format!(
                " {spin} {percent:>3}% [{}] {done}/{total}",
                bar(done, total, width_of_bar)
            );
            let full = format!("{base}{counters}{remaining}");
            if width == usize::MAX || full.chars().count() <= width || width_of_bar <= 8 {
                break if width_of_bar <= 10 && full.chars().count() > width {
                    format!("{base}{remaining}")
                } else {
                    full
                };
            }
            if width_of_bar > 10 {
                width_of_bar = 10;
            } else {
                break format!("{base}{remaining}");
            }
        };

        let tail = self
            .tail
            .lock()
            .map(|tail| tail.clone())
            .unwrap_or_default();
        if tail.is_empty() {
            return clip(&base, width);
        }
        if width == usize::MAX {
            return format!("{base}  {tail}");
        }
        let spare = width.saturating_sub(base.chars().count() + 2);
        if spare == 0 {
            return clip(&base, width);
        }
        format!("{}  {}", clip(&base, width - spare), clip(&tail, spare))
    }
}

/// What the last finished node did, in one short phrase.
fn describe(checked: &Checked) -> String {
    match &checked.status {
        Status::Published => match checked.mbps {
            Some(mbps) => format!("{}  {} Mbit/s", checked.node.address, human_mbps(mbps)),
            None => checked.node.address.clone(),
        },
        Status::TooSlow { mbps, floor } => format!(
            "{}  {} < {} Mbit/s",
            checked.node.address,
            human_mbps(*mbps),
            human_mbps(*floor)
        ),
        Status::Blacklisted { address, .. } => {
            format!("{}  blacklisted {address}", checked.node.address)
        }
        Status::Dead { reason } => format!("{}  {reason}", checked.node.address),
    }
}

/// Progress through the measured candidates.
pub struct Progress {
    state: Arc<State>,
    painter: Mutex<Option<JoinHandle<()>>>,
    interactive: bool,
    quiet: bool,
    last_step: AtomicUsize,
}

impl Progress {
    pub fn new(total: usize, quiet: bool) -> Self {
        let interactive = std::io::stderr().is_terminal() && !quiet;
        let state = Arc::new(State {
            total: AtomicUsize::new(total),
            done: AtomicUsize::new(0),
            published: AtomicUsize::new(0),
            slow: AtomicUsize::new(0),
            dead: AtomicUsize::new(0),
            timeout: AtomicUsize::new(0),
            error: AtomicUsize::new(0),
            blacklisted: AtomicUsize::new(0),
            frame: AtomicUsize::new(0),
            began: Instant::now(),
            tail: Mutex::new(String::new()),
            stopped: AtomicBool::new(false),
        });

        let painter = if interactive {
            let state = Arc::clone(&state);
            Some(std::thread::spawn(move || {
                let width = terminal_width();
                let mut first = true;
                while !state.stopped.load(Ordering::Relaxed) {
                    state.frame.fetch_add(1, Ordering::Relaxed);
                    let stat = state.stat_line();
                    let line = state.line(width);
                    if let Ok(_guard) = OUT.lock() {
                        if first {
                            eprint!("\r\x1b[2K{stat}\n\r\x1b[2K{line}");
                            first = false;
                        } else {
                            eprint!("\x1b[1A\r\x1b[2K{stat}\n\r\x1b[2K{line}");
                        }
                        let _ = std::io::stderr().flush();
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }))
        } else {
            None
        };

        Self {
            state,
            painter: Mutex::new(painter),
            interactive,
            quiet,
            last_step: AtomicUsize::new(0),
        }
    }

    /// Account for one finished node.
    pub fn record(&self, checked: &Checked) {
        self.bump(&checked.status, true);
        let done = self.state.done.fetch_add(1, Ordering::Relaxed) + 1;
        self.announce(checked, done);
    }

    /// Plan more attempts, so the bar still ends at one hundred percent.
    pub fn grow_total(&self, extra: usize) {
        self.state.total.fetch_add(extra, Ordering::Relaxed);
    }

    /// Account for an attempt that replaces an earlier verdict.
    ///
    /// The retry pass re-probes the nodes that did not answer, so the old
    /// verdict is taken back before the new one is counted: otherwise a rescued
    /// node would be counted both dead and published.
    pub fn retry(&self, before: &Status, after: &Checked) {
        self.bump(before, false);
        self.bump(&after.status, true);
        let done = self.state.done.fetch_add(1, Ordering::Relaxed) + 1;
        self.announce(after, done);
    }

    fn bump(&self, status: &Status, up: bool) {
        let counters: Vec<&AtomicUsize> = match status {
            Status::Published => vec![&self.state.published],
            Status::TooSlow { .. } => vec![&self.state.slow],
            Status::Blacklisted { .. } => vec![&self.state.blacklisted],
            Status::Dead { reason } => {
                let r = reason.to_lowercase();
                let is_timeout = r.contains("timeout") || r.contains("timed out") || r.contains("did not start");
                if is_timeout { vec![&self.state.dead, &self.state.timeout] } else { vec![&self.state.dead, &self.state.error] }
            }
        };
        for counter in counters {
            if up {
                counter.fetch_add(1, Ordering::Relaxed);
            } else {
                #[allow(deprecated)]
                let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| Some(count.saturating_sub(1)));
            }
        }
    }

    fn announce(&self, checked: &Checked, done: usize) {
        if let Ok(mut slot) = self.state.tail.lock() {
            *slot = describe(checked);
        }
        if self.interactive || self.quiet {
            return;
        }
        let total = self.state.total.load(Ordering::Relaxed);
        let percent = percent_of(done, total);
        let last = self.last_step.load(Ordering::Relaxed);
        if (percent >= last + 5 || done == total)
            && self
                .last_step
                .compare_exchange(last, percent, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            if let Ok(_guard) = OUT.lock() {
                eprintln!("{}", self.state.line(usize::MAX));
            }
        }
    }

    /// Stop the painter and print the final one-line summary.
    pub fn finish(&self) {
        self.state.stopped.store(true, Ordering::Relaxed);
        if let Ok(mut painter) = self.painter.lock() {
            if let Some(handle) = painter.take() {
                let _ = handle.join();
            }
        }
        if self.quiet {
            return;
        }
        if self.interactive {
            if let Ok(_guard) = OUT.lock() {
                eprint!("\r\x1b[2K\x1b[1A\r\x1b[2K");
                let _ = std::io::stderr().flush();
            }
        }
        eprintln!(
            "probed {} time(s) in {}: {} published, {} slow, {} dead, {} blacklisted",
            self.done(),
            human_duration(self.state.began.elapsed()),
            self.published(),
            self.slow(),
            self.dead(),
            self.blacklisted()
        );
    }

    pub fn done(&self) -> usize {
        self.state.done.load(Ordering::Relaxed)
    }

    pub fn published(&self) -> usize {
        self.state.published.load(Ordering::Relaxed)
    }

    /// Nodes that answered through a core, including those below the floor.
    pub fn answers(&self) -> usize {
        self.published() + self.slow()
    }

    pub fn slow(&self) -> usize {
        self.state.slow.load(Ordering::Relaxed)
    }

    pub fn dead(&self) -> usize {
        self.state.dead.load(Ordering::Relaxed)
    }

    pub fn blacklisted(&self) -> usize {
        self.state.blacklisted.load(Ordering::Relaxed)
    }
}

struct FetchState {
    total: AtomicUsize,
    done: AtomicUsize,
    frame: AtomicUsize,
    name: Mutex<String>,
    stopped: AtomicBool,
}

impl FetchState {
    fn line(&self) -> String {
        let done = self.done.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);
        let spin = SPIN[self.frame.load(Ordering::Relaxed) % SPIN.len()];
        let name = self
            .name
            .lock()
            .map(|name| name.clone())
            .unwrap_or_default();
        if name.is_empty() {
            format!(" {spin} fetching sources {done}/{total}")
        } else {
            format!(" {spin} fetching sources {done}/{total}  {name}")
        }
    }
}

/// A spinner for the download phase, before the number of candidates is known.
pub struct FetchProgress {
    state: Arc<FetchState>,
    painter: Mutex<Option<JoinHandle<()>>>,
    interactive: bool,
    quiet: bool,
}

impl FetchProgress {
    pub fn new(total: usize, quiet: bool) -> Self {
        let interactive = std::io::stderr().is_terminal() && !quiet;
        let state = Arc::new(FetchState {
            total: AtomicUsize::new(total),
            done: AtomicUsize::new(0),
            frame: AtomicUsize::new(0),
            name: Mutex::new(String::new()),
            stopped: AtomicBool::new(false),
        });
        let painter = if interactive {
            let state = Arc::clone(&state);
            Some(std::thread::spawn(move || {
                while !state.stopped.load(Ordering::Relaxed) {
                    state.frame.fetch_add(1, Ordering::Relaxed);
                    let line = state.line();
                    if let Ok(_guard) = OUT.lock() {
                        eprint!("\r\x1b[2K{line}");
                        let _ = std::io::stderr().flush();
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }))
        } else {
            None
        };
        Self {
            state,
            painter: Mutex::new(painter),
            interactive,
            quiet,
        }
    }

    /// One source finished; its name is shown until the next one does.
    pub fn done(&self, name: &str) {
        let done = self.state.done.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut slot) = self.state.name.lock() {
            *slot = name.to_string();
        }
        if !self.interactive && !self.quiet {
            let total = self.state.total.load(Ordering::Relaxed);
            if done == total || done % 10 == 0 {
                eprintln!("fetched {done}/{total} sources");
            }
        }
    }

    pub fn finish(&self) {
        self.state.stopped.store(true, Ordering::Relaxed);
        if let Ok(mut painter) = self.painter.lock() {
            if let Some(handle) = painter.take() {
                let _ = handle.join();
            }
        }
        if self.interactive {
            if let Ok(_guard) = OUT.lock() {
                eprint!("\r\x1b[2K");
                let _ = std::io::stderr().flush();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(status: Status) -> Checked {
        Checked {
            node: v2keylib::links::parse(
                "vless://00000000-0000-0000-0000-000000000000@192.0.2.1:443?security=tls&sni=example.com#fixture",
            )
            .expect("fixture link"),
            source: "fixture".into(),
            exit_ip: None,
            country: None,
            latency: None,
            mbps: Some(1.0),
            status,
        }
    }

    #[test]
    fn bar_fills_with_progress() {
        assert_eq!(bar(0, 10, 10), "..........");
        assert_eq!(bar(5, 10, 10), "#####.....");
        assert_eq!(bar(10, 10, 10), "##########");
        assert_eq!(bar(20, 10, 10), "##########");
    }

    #[test]
    fn percent_handles_an_unknown_total() {
        assert_eq!(percent_of(0, 0), 100);
        assert_eq!(percent_of(5, 10), 50);
        assert_eq!(percent_of(20, 10), 100);
    }

    #[test]
    fn a_retry_moves_a_node_between_counters() {
        let progress = Progress::new(1, true);
        let dead = checked(Status::Dead {
            reason: "timeout".into(),
        });
        progress.record(&dead);
        assert_eq!((progress.dead(), progress.published()), (1, 0));

        progress.grow_total(1);
        let live = checked(Status::Published);
        progress.retry(&dead.status, &live);
        assert_eq!((progress.dead(), progress.published()), (0, 1));
        assert_eq!(progress.done(), 2);
    }

    #[test]
    fn clip_does_not_split_a_flag() {
        assert_eq!(clip("🇳🇱 Netherlands", 3), "🇳🇱 ");
    }

    #[test]
    fn eta_is_only_shown_while_work_remains() {
        assert!(eta(0, 10, Duration::from_secs(5)).is_empty());
        assert!(eta(10, 10, Duration::from_secs(5)).is_empty());
        assert!(eta(5, 10, Duration::from_secs(10)).contains("10s"));
    }
}
