use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dyndo_core::storage::Storage;
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
    /// Build or update an asset from one or more media tracks.
    Index(commands::index::IndexArgs),
}

impl Command {
    async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Index(args) => commands::index::run(args).await,
        }
    }
}

fn init_storage() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var("OPENDAL_FS_ROOT").unwrap_or_else(|_| ".".to_owned());
    Storage::init(Fs::default().root(&root))?;

    Ok(())
}

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
    init_storage()?;
    cli.command.run().await
}
