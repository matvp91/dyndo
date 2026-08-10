use std::process::ExitCode;

use clap::{Parser, Subcommand};
use opendal::Operator;
use opendal::services::Fs;

mod commands;

/// dyndo — dynamic media packaging for adaptive streaming.
#[derive(Parser)]
#[command(name = "dyndo", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build or update an asset descriptor from one or more media tracks.
    Index(commands::index::IndexArgs),
    /// Extract a video frame as a JPEG image.
    Image(commands::image::ImageArgs),
}

impl Command {
    async fn run(self, op: &Operator) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Index(args) => commands::index::run(op, args).await,
            Self::Image(args) => commands::image::run(op, args).await,
        }
    }
}

fn operator() -> Result<Operator, Box<dyn std::error::Error>> {
    let root = std::env::var("OPENDAL_FS_ROOT").unwrap_or_else(|_| ".".to_string());
    Ok(Operator::new(Fs::default().root(&root))?)
}

// Print Display because Rust's default main error handling prints Debug.
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
