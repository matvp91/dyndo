use clap::{Args, Parser, Subcommand};
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
    Dash(DashArgs),
    /// Generate HLS playlists from an asset.json.
    Hls(HlsArgs),
}

#[derive(Args)]
struct SegmentArgs {
    /// Minimum served segment length in milliseconds.
    #[arg(long = "segment-min-length", default_value_t = 0)]
    min_length: u32,
    /// Length of each segment of a packaged subtitle track, in milliseconds. Zero
    /// cuts one only at the asset's splice points.
    #[arg(long = "segment-text-length", default_value_t = 0)]
    text_length: u32,
    /// Times a segment has to start at, in milliseconds:
    /// `--segment-boundaries 30000,60000`.
    #[arg(long = "segment-boundaries", value_delimiter = ',')]
    boundaries: Vec<u32>,
}

impl SegmentArgs {
    /// Assigns the options these flags name onto `options`, leaving the rest as the
    /// asset asked for them. A flag left at zero — or no boundaries at all — names
    /// nothing.
    fn assign_to(&self, options: &mut SegmentOptions) {
        if self.min_length != 0 {
            options.min_length = self.min_length;
        }
        if self.text_length != 0 {
            options.text_length = self.text_length;
        }
        if !self.boundaries.is_empty() {
            options.boundaries = self.boundaries.clone();
        }
    }
}

#[derive(Args)]
struct DashArgs {
    /// Input asset.json path.
    #[arg(short, long = "input")]
    input: String,
    /// Output manifest path.
    #[arg(short, long = "output", default_value = "stream.mpd")]
    output: String,
    #[command(flatten)]
    segment: SegmentArgs,
    /// Hoist common segment information in the MPD.
    #[arg(long, default_value_t = false)]
    compact: bool,
}

#[derive(Args)]
struct HlsArgs {
    /// Input asset.json path.
    #[arg(short, long = "input")]
    input: String,
    /// Output playlist directory.
    #[arg(short, long = "output", default_value = "hls")]
    output: String,
    #[command(flatten)]
    segment: SegmentArgs,
    /// Point text renditions at packaged wvtt segments rather than WebVTT
    /// documents.
    #[arg(long, default_value_t = false)]
    wvtt: bool,
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

                let track = Track::probe(&op, &path, None, &descriptor.segment_options).await?;
                input.apply(&mut descriptor.add_track(&track).kind);
            }

            op.write(&output, serde_json::to_vec_pretty(&descriptor)?)
                .await?;
            println!("wrote {output} ({} tracks)", descriptor.tracks.len());
        }
        Command::Dash(args) => {
            let mut descriptor = AssetDescriptor::read(&op, &args.input).await?;
            args.segment.assign_to(&mut descriptor.segment_options);
            let dash_options = dyndo_dash::options::DashOptions {
                compact: args.compact,
            };
            let mpd = dyndo_dash::builder::generate_mpd(&op, &descriptor, &dash_options).await?;
            let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            let mut serializer = quick_xml::se::Serializer::new(&mut xml);
            serializer.indent(' ', 2);
            mpd.serialize(serializer)?;
            op.write(&args.output, xml.into_bytes()).await?;
            println!("wrote {}", args.output);
        }
        Command::Hls(args) => {
            let mut descriptor = AssetDescriptor::read(&op, &args.input).await?;
            args.segment.assign_to(&mut descriptor.segment_options);
            let output = RelativePathBuf::from(args.output);
            op.create_dir(&format!("{output}/")).await?;

            let hls_options = dyndo_hls::options::HlsOptions { wvtt: args.wvtt };
            let master =
                dyndo_hls::builder::generate_master_playlist(&op, &descriptor, &hls_options)
                    .await?;
            let master_path = output.join("master.m3u8");
            op.write(master_path.as_str(), master.to_string()).await?;
            println!("wrote {master_path}");

            for track in &descriptor.tracks {
                let playlist = dyndo_hls::builder::generate_media_playlist(
                    &op,
                    &descriptor,
                    track,
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
