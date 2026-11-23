//! Command line parsing without a parser crate.
//!
//! The surface is small and the help text is the documentation, so a hand-written
//! loop is easier to keep honest than a derive: every flag has exactly one
//! spelling, every option says what happens when it is absent, and an unknown
//! flag fails instead of being ignored.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::settings::Settings;

pub const HELP: &str = "\
NetheriteParser — subscription builder verified through mihomo and Xray

USAGE:
    netherite [OPTIONS]

INPUT:
    --config PATH           settings file (default: configuration.toml)
    --sources PATH          source list (default: sources.txt)
    --tg PATH               telegram channels list (default: tg.txt, optional)
    --blacklist PATH        IP/CIDR blacklist (default: blacklist.txt, optional)
    --max-nodes N           stop after N deduplicated candidates
    --per-source N          keep at most N candidates from each source
    --max-sources N         fetch at most N sources
    --resume                skip links already in <out>/dead.txt
    --retry-dead N          extra attempts for a node that did not answer (default: 0)

PROBING (every candidate runs through both cores):
    --concurrency N         processes at once per core (default: 6)
    --speed-concurrency N   simultaneous speed downloads (default: 1)
    --timeout SECS          handshake and exit lookup budget (default: 20)
    --exit-timeout SECS     exit lookup budget on its own (default: 20)
    --fetch-concurrency N   sources fetched at once (default: 8)
    --latency-url URL       URL used for latency (default: gstatic 204)
    --speed-url URL         speed-test URL; repeatable, replaces the defaults
    --speed-timeout SECS    per-URL download budget (default: 15)
    --speed-bytes N         bytes to ask for (default: 10000000)
    --threshold MBPS        speed floor (default: 15)
    --no-speed              do not measure or filter by speed

OUTPUT:
    --out DIR               output directory (default: sub)
    --dry-run               fetch and merge only; write nothing
    --quiet                 only errors and the final summary

OTHER:
    --seed N                make the four-digit suffixes reproducible
    -h, --help              this text
    -V, --version           print the version

The published files are:
    <out>/default           link subscription, nodes that passed Xray
    <out>/mihomo            mihomo YAML, nodes that passed mihomo
    <out>/meta.json         versions and counts for this run
    <out>/dead.txt          rejected links, one per line
    <out>/source_report.txt per-source usefulness
";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug)]
pub enum Action {
    Run(Box<Settings>),
    Help,
    Version,
}

/// The file read when `--config` is not given.
pub const DEFAULT_CONFIG: &str = "configuration.toml";

pub fn parse<I>(args: I) -> Result<Action>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    parse_with(args, Some(Path::new(DEFAULT_CONFIG)))
}

/// The same parser with the default file made explicit.
///
/// Tests pass `None` so a developer's `configuration.toml` cannot change what a
/// test asserts.
fn parse_with(args: Vec<String>, fallback: Option<&Path>) -> Result<Action> {
    // These two do not need a valid configuration file.
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return Ok(Action::Help);
    }
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        return Ok(Action::Version);
    }

    let explicit = config_path(&args);
    let mut settings = match (&explicit, fallback) {
        (Some(path), _) => Settings::from_config(path)?,
        (None, Some(path)) if path.exists() => Settings::from_config(path)?,
        _ => Settings::default(),
    };
    let mut speed_urls_given = false;
    let mut index = 0;

    while index < args.len() {
        let arg = args[index].clone();
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_string(), Some(value.to_string()))
            }
            _ => (arg, None),
        };

        // Every option below wants a value; the closure keeps the error message
        // the same shape for all of them.
        let mut value = |name: &str| -> Result<String> {
            if let Some(value) = inline.clone() {
                return Ok(value);
            }
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| Error::Config(format!("{name} needs a value")))
        };

        match flag.as_str() {
            "--config" => {
                value("--config")?;
            }
            "--sources" => settings.sources = PathBuf::from(value("--sources")?),
            "--tg" => settings.tg = PathBuf::from(value("--tg")?),
            "--blacklist" => settings.blacklist = PathBuf::from(value("--blacklist")?),
            "--out" => settings.out_dir = PathBuf::from(value("--out")?),
            "--max-nodes" => {
                settings.max_nodes = Some(number(&value("--max-nodes")?, "--max-nodes")?)
            }
            "--per-source" => {
                settings.per_source = number(&value("--per-source")?, "--per-source")?
            }
            "--max-sources" => {
                settings.max_sources = number(&value("--max-sources")?, "--max-sources")?
            }
            "--concurrency" => {
                settings.concurrency = number(&value("--concurrency")?, "--concurrency")?
            }
            "--speed-concurrency" => {
                settings.speed_concurrency =
                    number(&value("--speed-concurrency")?, "--speed-concurrency")?
            }
            "--fetch-concurrency" => {
                settings.fetch_concurrency =
                    number(&value("--fetch-concurrency")?, "--fetch-concurrency")?
            }
            "--timeout" => settings.node_timeout = seconds(&value("--timeout")?, "--timeout")?,
            "--exit-timeout" => {
                settings.exit_timeout = seconds(&value("--exit-timeout")?, "--exit-timeout")?
            }
            "--latency-url" => settings.latency_url = value("--latency-url")?,
            "--latency-timeout" => {
                settings.latency_timeout =
                    seconds(&value("--latency-timeout")?, "--latency-timeout")?
            }
            "--speed-url" => {
                if !speed_urls_given {
                    settings.speed_urls.clear();
                    speed_urls_given = true;
                }
                settings.speed_urls.push(value("--speed-url")?);
            }
            "--speed-timeout" => {
                settings.speed_timeout = seconds(&value("--speed-timeout")?, "--speed-timeout")?
            }
            "--speed-bytes" => {
                settings.speed_bytes = number(&value("--speed-bytes")?, "--speed-bytes")?
            }
            "--threshold" => {
                let raw = value("--threshold")?;
                settings.threshold_mbps = raw
                    .parse()
                    .map_err(|_| Error::Config(format!("--threshold: not a number: {raw}")))?;
            }
            "--seed" => settings.seed = value("--seed")?.parse().unwrap_or(0),
            "--no-speed" => settings.speed_enabled = false,
            "--dry-run" => settings.dry_run = true,
            "--resume" => settings.resume = true,
            "--retry-dead" => {
                settings.retry_dead = number(&value("--retry-dead")?, "--retry-dead")?
            }
            "--quiet" => settings.quiet = true,
            other => {
                return Err(Error::Config(format!(
                    "unknown option: {other} (try --help)"
                )))
            }
        }
        index += 1;
    }

    if settings.fetch_concurrency == 0 {
        settings.fetch_concurrency = 1;
    }
    Ok(Action::Run(Box::new(settings)))
}

/// The path given with `--config`, either spelling.
fn config_path(args: &[String]) -> Option<PathBuf> {
    args.iter()
        .position(|arg| arg == "--config")
        .and_then(|at| args.get(at + 1))
        .map(PathBuf::from)
        .or_else(|| {
            args.iter()
                .find_map(|arg| arg.strip_prefix("--config=").map(PathBuf::from))
        })
}

fn number(raw: &str, flag: &str) -> Result<usize> {
    raw.parse()
        .map_err(|_| Error::Config(format!("{flag}: not a whole number: {raw}")))
}

fn seconds(raw: &str, flag: &str) -> Result<Duration> {
    raw.parse::<u64>()
        .map(Duration::from_secs)
        .map_err(|_| Error::Config(format!("{flag}: not a number of seconds: {raw}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Settings {
        match parse_with(args.iter().map(|s| s.to_string()).collect(), None).unwrap() {
            Action::Run(settings) => *settings,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn defaults_are_a_finished_run() {
        let settings = run(&[]);
        assert_eq!(settings.threshold_mbps, 15.0);
        assert!(settings.speed_enabled);
        assert_eq!(settings.out_dir, PathBuf::from("sub"));
    }

    #[test]
    fn repeated_speed_urls_replace_the_defaults() {
        let settings = run(&["--speed-url", "https://a", "--speed-url", "https://b"]);
        assert_eq!(settings.speed_urls, vec!["https://a", "https://b"]);
    }

    #[test]
    fn inline_value_is_accepted() {
        assert_eq!(run(&["--max-nodes=12"]).max_nodes, Some(12));
    }

    #[test]
    fn fetch_concurrency_is_configurable() {
        assert_eq!(run(&["--fetch-concurrency=3"]).fetch_concurrency, 3);
        assert_eq!(run(&["--fetch-concurrency", "0"]).fetch_concurrency, 1);
    }

    #[test]
    fn speed_downloads_are_serialized_by_default() {
        assert_eq!(run(&[]).speed_concurrency, 1);
        assert_eq!(run(&["--speed-concurrency=2"]).speed_concurrency, 2);
    }

    #[test]
    fn dead_nodes_are_not_retried_by_default() {
        assert_eq!(run(&[]).retry_dead, 0);
        assert_eq!(run(&["--retry-dead=1"]).retry_dead, 1);
        assert_eq!(run(&["--retry-dead", "2"]).retry_dead, 2);
    }

    #[test]
    fn a_config_file_is_read_and_flags_win() {
        let dir = std::env::temp_dir().join(format!("netherite-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("configuration.toml");
        std::fs::write(
            &file,
            "[limits]\nmax_nodes = 7\nconcurrency = 3\n[thresholds]\nspeed_mbps = 9.5\n\
             [identity]\nname = \"Rock\"\nannounce = \"hi\"\n",
        )
        .unwrap();

        let path = file.to_str().unwrap();
        let settings =
            match parse_with(vec!["--config".to_string(), path.to_string()], None).unwrap() {
                Action::Run(settings) => *settings,
                other => panic!("{other:?}"),
            };
        assert_eq!(settings.max_nodes, Some(7));
        assert_eq!(settings.concurrency, 3);
        assert_eq!(settings.threshold_mbps, 9.5);
        assert_eq!(settings.name, "Rock");
        assert_eq!(settings.announce.as_deref(), Some("hi"));

        let settings = match parse_with(
            vec![
                "--config".to_string(),
                path.to_string(),
                "--threshold".to_string(),
                "12".to_string(),
                "--max-nodes".to_string(),
                "5".to_string(),
            ],
            None,
        )
        .unwrap()
        {
            Action::Run(settings) => *settings,
            other => panic!("{other:?}"),
        };
        assert_eq!(settings.max_nodes, Some(5));
        assert_eq!(settings.threshold_mbps, 12.0);
        assert_eq!(settings.name, "Rock");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_explicit_config_is_an_error() {
        assert!(parse_with(
            vec![
                "--config".to_string(),
                "/nonexistent/configuration.toml".to_string()
            ],
            None,
        )
        .is_err());
    }

    #[test]
    fn unknown_flag_fails() {
        assert!(parse(["--nope".to_string()]).is_err());
    }

    #[test]
    fn speed_floor_follows_a_slow_line() {
        let settings = run(&[]);
        assert_eq!(settings.effective_threshold(Some(46.3)), 15.0);
        assert_eq!(settings.effective_threshold(Some(15.0)), 15.0);
        assert_eq!(settings.effective_threshold(Some(10.0)), 7.5);
    }
}
