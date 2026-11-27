use std::sync::Mutex;
use std::time::Duration;

pub const BAR_WIDTH: usize = 24;
pub const SPIN: [&str; 4] = ["|", "/", "-", "\\"];
pub static OUT: Mutex<()> = Mutex::new(());

pub fn terminal_width() -> usize {
    if let Ok(text) = std::env::var("COLUMNS") {
        if let Ok(value) = text.trim().parse::<usize>() {
            if value > 20 { return value; }
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

pub fn percent_of(done: usize, total: usize) -> usize {
    if total == 0 { return 100; }
    done.min(total) * 100 / total
}

pub fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width { return text.to_string(); }
    text.chars().take(width).collect()
}

pub fn bar(done: usize, total: usize, width: usize) -> String {
    let filled = width * done.min(total) / total.max(1);
    format!("{}{}", "#".repeat(filled.min(width)), ".".repeat(width - filled.min(width)))
}

pub fn eta(done: usize, total: usize, elapsed: Duration) -> String {
    if done == 0 || done >= total { return String::new(); }
    let each = elapsed.as_secs_f64() / done as f64;
    let left = (each * (total - done) as f64).round() as u64;
    if left < 60 { format!("  eta {left}s") } else { format!("  eta {}m{:02}s", left / 60, left % 60) }
}
