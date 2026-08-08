use std::process::ExitCode;

use clap::Parser;
use opendal::Operator;
use opendal::services::Fs;

mod commands;

use commands::Command;

/// dyndo — dynamic media packaging for adaptive streaming.
#[derive(Parser)]
#[command(name = "dyndo", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Build the filesystem operator, rooted at `OPENDAL_FS_ROOT` (default `.`).
fn operator() -> Result<Operator, Box<dyn std::error::Error>> {
    let root = std::env::var("OPENDAL_FS_ROOT").unwrap_or_else(|_| ".".to_string());
    Ok(Operator::new(Fs::default().root(&root))?)
}

/// Reports what went wrong rather than how it is spelled in Rust: returning the
/// error from `main` would print its `Debug`, which quotes a message or, for a unit
/// error like a filter matching nothing, drops it entirely.
#[tokio::main]
async fn main() -> ExitCode {
    if let Err(error) = run().await {
        eprintln!("dyndo: {error}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let op = operator()?;
    cli.command.run(&op).await
}
