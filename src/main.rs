//! The binary: parse arguments, run, print the summary.

use std::process::ExitCode;

use netherite::cli::{self, Action};
use netherite::pipeline;
use netherite::report::Summary;

#[tokio::main]
async fn main() -> ExitCode {
    let action = match cli::parse(std::env::args().skip(1)) {
        Ok(action) => action,
        Err(e) => {
            eprintln!("netherite: {e}");
            return ExitCode::from(2);
        }
    };

    match action {
        Action::Help => {
            print!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("netherite {}", cli::VERSION);
            ExitCode::SUCCESS
        }
        Action::Run(settings) => match pipeline::run(*settings).await {
            Ok(Some(report)) => {
                let summary = Summary {
                    outcome: &report.outcome,
                    stats: &report.stats,
                    files: &report.files,
                    sources_total: report.sources_total,
                    sources_used: report.sources_used,
                    duplicates: report.duplicates,
                    truncated: report.truncated,
                };
                println!("{}", summary.render());
                ExitCode::SUCCESS
            }
            Ok(None) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("netherite: {e}");
                ExitCode::FAILURE
            }
        },
    }
}
