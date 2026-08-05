use clap::{Parser, Subcommand};
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::Track;
use opendal::Operator;
use opendal::services::Fs;
use relative_path::{RelativePath, RelativePathBuf};
use serde::Serialize;

mod track_input;

use track_input::TrackInput;

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
        inputs: Vec<TrackInput>,
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
    },
    /// Generate HLS playlists from an asset.json.
    Hls {
        /// Input asset.json path.
        #[arg(short, long = "input", default_value = "asset.json")]
        input: String,
        /// Output playlist directory.
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
            let output_path = RelativePathBuf::from(output.as_str());
            let output_base = output_path.parent().unwrap_or(RelativePath::new(""));
            let mut descriptor = AssetDescriptor::read_or_new(&op, &output_path).await?;

            for input in inputs {
                let path = output_base.join(&input.path);
                if let Some(track) = descriptor.find_track_mut(&path) {
                    input.apply(&mut track.kind);
                    continue;
                }

                let track = Track::probe(&op, &path, None).await?;
                input.apply(&mut descriptor.add_track(&track).kind);
            }

            op.write(&output, serde_json::to_vec_pretty(&descriptor)?)
                .await?;
            println!("wrote {output} ({} tracks)", descriptor.tracks.len());
        }
        Command::Dash { input, output } => {
            let descriptor = AssetDescriptor::read(&op, &input).await?;
            let mpd = dyndo_dash::builder::generate_mpd(
                &op,
                &descriptor,
                &SegmentOptions::default(),
                &dyndo_dash::options::DashOptions::default(),
            )
            .await?;
            let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            let mut serializer = quick_xml::se::Serializer::new(&mut xml);
            serializer.indent(' ', 2);
            mpd.serialize(serializer)?;
            op.write(&output, xml.into_bytes()).await?;
            println!("wrote {output}");
        }
        Command::Hls { input, output } => {
            let descriptor = AssetDescriptor::read(&op, &input).await?;
            let output = RelativePathBuf::from(output);
            op.create_dir(&format!("{output}/")).await?;

            let segment_options = SegmentOptions::default();
            let hls_options = dyndo_hls::options::HlsOptions::default();
            let master = dyndo_hls::builder::generate_master_playlist(
                &op,
                &descriptor,
                &segment_options,
                &hls_options,
            )
            .await?;
            let master_path = output.join("master.m3u8");
            op.write(master_path.as_str(), master.to_string()).await?;
            println!("wrote {master_path}");

            for track in &descriptor.tracks {
                let playlist = dyndo_hls::builder::generate_media_playlist(
                    &op,
                    &descriptor,
                    track,
                    &segment_options,
                    &hls_options,
                )
                .await?;
                let path = output.join(format!("{}.m3u8", track.id));
                op.write(
                    path.as_str(),
                    dyndo_hls::builder::serialize_media_playlist(&playlist),
                )
                .await?;
                println!("wrote {path}");
            }
        }
    }
    Ok(())
}
