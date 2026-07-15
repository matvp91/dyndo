use std::path::Path;

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
    /// Generate HLS playlists (a multivariant playlist + one media playlist per
    /// track) from an asset.json, into an output directory.
    Hls {
        /// Input asset.json path.
        #[arg(short, long = "input", default_value = "asset.json")]
        input: String,
        /// Output directory for the playlists.
        #[arg(short, long = "output", default_value = "hls")]
        output: String,
    },
    /// Pack a source subtitle/text file into a CMAF track. The input's
    /// extension selects the packer (currently: `.vtt` → `wvtt`).
    Pack {
        /// Input source file (currently `.vtt`).
        #[arg(short, long = "input")]
        input: String,
        /// Output CMAF track path.
        #[arg(short, long = "output", default_value = "text.mp4")]
        output: String,
        /// Segment duration, in milliseconds.
        #[arg(short = 'd', long = "segment-duration", default_value_t = 4000)]
        segment_duration_ms: u64,
        /// ISO-639-2 language code stored in the track.
        #[arg(short, long = "language", default_value = "und")]
        language: String,
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
                asset.add_track(&op, path, &output).await?;
            }
            asset.path = output;
            AssetModel::from(&asset).write(&op, &asset.path).await?;
            let tracks =
                asset.video_tracks.len() + asset.audio_tracks.len() + asset.text_tracks.len();
            println!("wrote {} ({tracks} tracks)", asset.path);
        }
        Command::Dash {
            input,
            output,
            compact,
        } => {
            let model = AssetModel::read(&op, &input).await?;
            let asset = Asset::from_model(&op, model, &input).await?;
            let mpd = dyndo_core::dash::generate_mpd(&asset, compact);
            op.write(&output, mpd.into_bytes()).await?;
            println!("wrote {output}");
        }
        Command::Hls { input, output } => {
            let model = AssetModel::read(&op, &input).await?;
            let asset = Asset::from_model(&op, model, &input).await?;
            op.write(
                &format!("{output}/index.m3u8"),
                dyndo_core::hls::generate_master(&asset).into_bytes(),
            )
            .await?;
            let count = asset.video_tracks.len() + asset.audio_tracks.len();
            for t in &asset.video_tracks {
                op.write(
                    &format!("{output}/{}.m3u8", t.id()),
                    dyndo_core::hls::generate_media(t).into_bytes(),
                )
                .await?;
            }
            for t in &asset.audio_tracks {
                op.write(
                    &format!("{output}/{}.m3u8", t.id()),
                    dyndo_core::hls::generate_media(t).into_bytes(),
                )
                .await?;
            }
            println!("wrote {output}/ (1 master + {count} media)");
        }
        Command::Pack {
            input,
            output,
            segment_duration_ms,
            language,
        } => {
            let ext = Path::new(&input)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            match ext.as_deref() {
                Some("vtt") => {
                    let raw = op.read(&input).await?;
                    let text = String::from_utf8(raw.to_vec())
                        .map_err(|e| format!("input is not valid UTF-8: {e}"))?;
                    let mut subtitle = dyndo_core::text::parse(&text)?;
                    subtitle.language = language;
                    let bytes = dyndo_core::text::wvtt::pack(&subtitle, segment_duration_ms)?;
                    op.write(&output, bytes).await?;
                    println!("wrote {output}");
                }
                other => {
                    return Err(format!(
                        "pack: unsupported input extension {other:?} (supported: vtt)"
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}
