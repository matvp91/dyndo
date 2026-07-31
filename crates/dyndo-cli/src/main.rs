use clap::{Parser, Subcommand};
use dyndo_core::asset::Asset as LegacyAsset;
use dyndo_core::metadata::Metadata;
use dyndo_core::next::asset::Asset;
use opendal::Operator;
use opendal::services::Fs;

mod track_descriptor;

/// dyndo — dynamic media packaging for adaptive streaming.
#[derive(Parser)]
#[command(name = "dyndo", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build or update an asset.json descriptor from one or more track
    /// descriptors. Each descriptor is `<path>[,language=..][,role=..]`, where
    /// the path is relative to the output descriptor's directory. New tracks
    /// are probed from their file; tracks already in the descriptor keep
    /// their metadata as-is, with only explicit overrides applied.
    Index {
        /// Track descriptor(s): `<path>[,language=..][,role=..]`, one per track.
        #[arg(required = true)]
        inputs: Vec<String>,
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
    /// Generate HLS playlists (a multivariant playlist + one media playlist per
    /// advertised track) from an asset.json, into an output directory.
    Hls {
        /// Input asset.json path.
        #[arg(short, long = "input", default_value = "asset.json")]
        input: String,
        /// Output directory for the playlists.
        #[arg(short, long = "output", default_value = "hls")]
        output: String,
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
        Command::Index { inputs, output } => {
            let mut asset = Asset::read(&op, &output).await?;

            for input in &inputs {
                let (path, language, role) = track_descriptor::parse_track_descriptor(input)?;
                let track = asset.index_track(&op, &path).await?;
                track_descriptor::apply_overrides(track, language.as_deref(), role.as_deref())?;
            }
            asset.write(&op).await?;
            println!("wrote {output} ({} tracks)", asset.tracks.len());
        }
        Command::Dash {
            input,
            output,
            compact,
        } => {
            let asset = Asset::read(&op, &input).await?;
            let mpd = dyndo_core::next::dash::generate_mpd(&op, &asset, compact).await?;
            op.write(&output, mpd.into_bytes()).await?;
            println!("wrote {output}");
        }
        Command::Hls { input, output } => {
            let asset = LegacyAsset::read_with_headers(&op, &input).await?;
            op.write(
                &format!("{output}/index.m3u8"),
                dyndo_core::hls::generate_master(&asset).into_bytes(),
            )
            .await?;
            // Media playlists for the advertised tracks only: text tracks
            // are not part of this generation's playlists.
            let mut count = 0;
            for t in asset
                .tracks
                .iter()
                .filter(|t| matches!(t.metadata, Metadata::Video(_) | Metadata::Audio(_)))
            {
                op.write(
                    &format!("{output}/{}.m3u8", t.id),
                    dyndo_core::hls::generate_media(
                        t,
                        &asset.segment_boundaries_ms,
                        asset.min_segment_length_ms,
                    )
                    .into_bytes(),
                )
                .await?;
                count += 1;
            }
            println!("wrote {output}/ (1 master + {count} media)");
        }
    }
    Ok(())
}
