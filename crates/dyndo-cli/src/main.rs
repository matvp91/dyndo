use std::path::PathBuf;

use clap::Parser;

/// Build an asset.json descriptor from one or more CMAF files.
#[derive(Parser)]
#[command(name = "dyndo", version, about)]
struct Cli {
    /// Input CMAF file (repeatable, one track each).
    #[arg(short, long = "input", required = true)]
    input: Vec<PathBuf>,

    /// Output descriptor path.
    #[arg(short, long = "output", default_value = "asset.json")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let asset = dyndo_core::build_asset(&cli.input).await?;
    let json = serde_json::to_string_pretty(&asset)?;
    tokio::fs::write(&cli.output, json).await?;
    println!(
        "wrote {} ({} tracks)",
        cli.output.display(),
        asset.tracks.len()
    );
    Ok(())
}
