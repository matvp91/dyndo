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
    /// Pack a source subtitle file into a CMAF text track aligned to the first
    /// video track of an asset, writing it as `<id>.mp4` beside the descriptor and
    /// adding it to the asset. The input extension selects the packer
    /// (currently: `.vtt` → `wvtt`).
    Pack {
        /// Input source file (currently `.vtt`).
        #[arg(short, long = "input")]
        input: String,
        /// Asset descriptor to align to and update.
        #[arg(short, long = "asset", default_value = "asset.json")]
        asset: String,
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
            let count =
                asset.video_tracks.len() + asset.audio_tracks.len() + asset.text_tracks.len();
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
            for t in &asset.text_tracks {
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
            asset,
            language,
        } => {
            let ext = Path::new(&input)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            match ext.as_deref() {
                Some("vtt") => {
                    // First video track's segment timeline (error if no video).
                    let model = AssetModel::read(&op, &asset).await?;
                    let mut asset_obj = Asset::from_model(&op, model, &asset).await?;
                    let segments = asset_obj
                        .video_tracks
                        .first()
                        .ok_or_else(|| {
                            "pack: asset has no video track to align subtitles to".to_string()
                        })?
                        .segments();

                    // Parse → expand → pack.
                    let raw = op.read(&input).await?;
                    let text = String::from_utf8(raw.to_vec())
                        .map_err(|e| format!("input is not valid UTF-8: {e}"))?;
                    let subtitle = dyndo_core::text::vtt::parse(&text)?;
                    let language = if language.is_empty() {
                        "und".to_string()
                    } else {
                        language
                    };
                    let windows = subtitle.expand(segments);
                    let bytes = dyndo_core::text::wvtt::pack(&language, &windows, segments)?;

                    // Text ids are header-free (text_{fourcc}_{language}), so the
                    // name is known before writing. Mirrors
                    // dyndo_core::asset::TextTrack::id; packing is always wvtt.
                    let out = format!("text_wvtt_{language}.mp4");
                    let id = format!("text_wvtt_{language}");
                    let dest = dyndo_core::path::resolve(&asset, &out);
                    op.write(&dest, bytes).await?;

                    // Add to the model (add_track probes the file), replacing any
                    // stale same-id entry, then rewrite the descriptor.
                    asset_obj.text_tracks.retain(|t| t.id() != id);
                    asset_obj.add_track(&op, &out, &asset).await?;
                    AssetModel::from(&asset_obj).write(&op, &asset).await?;
                    println!("wrote {out}; updated {asset}");
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
