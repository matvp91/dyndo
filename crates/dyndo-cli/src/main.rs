use clap::{Parser, Subcommand};
use dyndo_core::asset::{Asset, Track};
use dyndo_core::model::AssetModel;
use opendal::services::Fs;
use opendal::Operator;

/// dyndo — CMAF indexer and DASH manifest generator.
#[derive(Parser)]
#[command(name = "dyndo", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build an asset.json descriptor from one or more CMAF files. Inputs are
    /// track paths relative to the output descriptor's directory.
    Index {
        /// Input CMAF file (repeatable, one track each).
        #[arg(short, long = "input", required = true)]
        input: Vec<String>,
        /// Output descriptor path.
        #[arg(short, long = "output", default_value = "asset.json")]
        output: String,
    },
    /// Generate a DASH MPD from an asset.json.
    Dash {
        /// Input asset.json path.
        #[arg(short, long = "input", default_value = "asset.json")]
        input: String,
        /// Output manifest path.
        #[arg(short, long = "output", default_value = "stream.mpd")]
        output: String,
        /// Hoist SegmentTemplate content shared by all Representations up to the
        /// AdaptationSet level.
        #[arg(short = 'c', long = "compact")]
        compact: bool,
    },
}

/// Build the filesystem operator, rooted at `OPENDAL_FS_ROOT` (default `.`).
fn operator() -> Result<Operator, Box<dyn std::error::Error>> {
    let root = std::env::var("OPENDAL_FS_ROOT").unwrap_or_else(|_| ".".to_string());
    Ok(Operator::new(Fs::default().root(&root))?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let op = operator()?;
    match cli.command {
        Command::Index { input, output } => {
            let mut asset = Asset::new();
            for path in &input {
                asset.add_track(Track::from_path(&op, path, &output).await?);
            }
            asset.path = output;
            AssetModel::from(&asset).write(&op, &asset.path).await?;
            println!("wrote {} ({} tracks)", asset.path, asset.tracks.len());
        }
        Command::Dash {
            input,
            output,
            compact,
        } => {
            let model = AssetModel::read(&op, &input).await?;
            let asset = Asset::from_model(&op, model, &input).await?;
            let mpd = dyndo_dash::generate_mpd(&asset, compact);
            op.write(&output, mpd.into_bytes()).await?;
            println!("wrote {output}");
        }
    }
    Ok(())
}
