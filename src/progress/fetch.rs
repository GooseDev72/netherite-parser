use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::util::{OUT, SPIN};

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
