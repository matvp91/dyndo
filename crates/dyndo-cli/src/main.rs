use clap::{Parser, Subcommand};
use dyndo_core::asset::Asset;
use dyndo_core::header::Header;
use dyndo_core::metadata::Metadata;
use dyndo_core::track::Track;
use opendal::Operator;
use opendal::services::Fs;

mod params;

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
            let mut asset = if op.exists(&output).await? {
                Asset::read(&op, &output).await?
            } else {
                let mut a = Asset::new();
                a.path = output.clone();
                a
            };
            for input in &inputs {
                let (path, language, role) = params::parse_track_descriptor(input)?;
                match asset.tracks.iter_mut().find(|t| t.path == path) {
                    // Already indexed: the descriptor's metadata is
                    // authoritative — keep it as-is, applying only the
                    // explicit overrides.
                    Some(existing) => {
                        params::apply_overrides(existing, language.as_deref(), role.as_deref())?;
                    }
                    // New track: populate its metadata from the file, then
                    // pin the derived id in the descriptor — segment routes
                    // key on it, so later metadata edits must not re-derive
                    // it.
                    None => {
                        let mut track = Track::read(&op, &output, &path).await?;
                        params::apply_overrides(&mut track, language.as_deref(), role.as_deref())?;
                        track.id = track.id();
                        asset.tracks.push(track);
                    }
                }
            }
            asset.write(&op, &output).await?;
            println!("wrote {output} ({} tracks)", asset.tracks.len());
        }
        Command::Dash {
            input,
            output,
            compact,
        } => {
            let asset = Asset::read(&op, &input).await?;
            let mpd = dyndo_core::dash::generate_mpd(&asset, compact);
            op.write(&output, mpd.into_bytes()).await?;
            println!("wrote {output}");
        }
        Command::Hls { input, output } => {
            let asset = Asset::read(&op, &input).await?;
            op.write(
                &format!("{output}/index.m3u8"),
                dyndo_core::hls::generate_master(&asset).into_bytes(),
            )
            .await?;
            // Media playlists for the advertised tracks only: text and raw
            // (non-CMAF) tracks are not part of this generation's playlists.
            let mut count = 0;
            for t in asset.tracks.iter().filter(|t| {
                matches!(t.header(), Header::Cmaf(_))
                    && matches!(t.metadata, Metadata::Video(_) | Metadata::Audio(_))
            }) {
                op.write(
                    &format!("{output}/{}.m3u8", t.id()),
                    dyndo_core::hls::generate_media(t).into_bytes(),
                )
                .await?;
                count += 1;
            }
            println!("wrote {output}/ (1 master + {count} media)");
        }
    }
    Ok(())
}
