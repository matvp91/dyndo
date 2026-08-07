mod dash;
mod hls;
mod index;
mod sprite;

use clap::{Args, Subcommand};
use dyndo_core::segment::SegmentOptions;
use opendal::Operator;

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Build or update an asset.json descriptor from one or more track
    /// descriptors. Each descriptor is `<path>[,language=..][,role=..]`, where
    /// the path is relative to the output descriptor's directory. New tracks
    /// are probed from their file; tracks already in the descriptor keep
    /// their metadata as-is, with only explicit overrides applied.
    Index(index::IndexArgs),
    /// Generate a DASH MPD from an asset.json.
    Dash(dash::DashArgs),
    /// Generate HLS playlists from an asset.json.
    Hls(hls::HlsArgs),
    /// Cut one thumbnail sprite sheet from an asset.json, decoding it out of the
    /// video rendition best suited to the thumbnail size asked for.
    Sprite(sprite::SpriteArgs),
}

impl Command {
    pub(crate) async fn run(self, op: &Operator) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Index(args) => index::run(op, args).await,
            Self::Dash(args) => dash::run(op, args).await,
            Self::Hls(args) => hls::run(op, args).await,
            Self::Sprite(args) => sprite::run(op, args).await,
        }
    }
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
