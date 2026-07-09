use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// dyndo — CMAF indexer and DASH manifest generator.
#[derive(Parser)]
#[command(name = "dyndo", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build an asset.json descriptor from one or more CMAF files.
    Index {
        /// Input CMAF file (repeatable, one track each).
        #[arg(short, long = "input", required = true)]
        input: Vec<PathBuf>,
        /// Output descriptor path.
        #[arg(short, long = "output", default_value = "asset.json")]
        output: PathBuf,
    },
    /// Generate a DASH MPD from an asset.json (sources resolved in the CWD).
    Dash {
        /// Input asset.json path.
        #[arg(short, long = "input", default_value = "asset.json")]
        input: PathBuf,
        /// Output manifest path.
        #[arg(short, long = "output", default_value = "stream.mpd")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Index { input, output } => {
            let asset = dyndo_core::build_asset(&input).await?;
            let json = serde_json::to_string_pretty(&asset)?;
            tokio::fs::write(&output, json).await?;
            println!("wrote {} ({} tracks)", output.display(), asset.tracks.len());
        }
        Command::Dash { input, output } => {
            let bytes = tokio::fs::read(&input).await?;
            let asset: dyndo_core::Asset = serde_json::from_slice(&bytes)?;
            let mpd = dyndo_core::generate_mpd(&asset).await?;
            tokio::fs::write(&output, mpd).await?;
            println!("wrote {}", output.display());
        }
    }
    Ok(())
}
