use clap::Args;
use dyndo_core::asset_descriptor::{AssetDescriptor, TrackKind};
use dyndo_core::track::Track;
use dyndo_image::sprite::Sprite;
use opendal::Operator;

#[derive(Args)]
pub(crate) struct SpriteArgs {
    /// Input asset.json path.
    #[arg(short, long = "input")]
    input: String,
    /// Output image path.
    #[arg(short, long = "output", default_value = "sprite.jpg")]
    output: String,
    /// Thumbnails per sprite row, and per sprite column.
    #[arg(long = "tile-size", value_parser = clap::value_parser!(u32).range(1..))]
    tile_size: u32,
    /// Milliseconds between one thumbnail and the next.
    #[arg(long)]
    step: u32,
    /// Presentation time the sprite's first thumbnail shows, in milliseconds. Each
    /// thumbnail after it steps on by `--step`.
    #[arg(long, default_value_t = 0)]
    time: u64,
}

pub(super) async fn run(op: &Operator, args: SpriteArgs) -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = AssetDescriptor::read(op, &args.input).await?;
    // A sprite comes out the size of the track it is cut from, and the first video
    // track is as good a choice as any until something asks for a better one.
    let track_descriptor = descriptor
        .tracks
        .iter()
        .find(|track| matches!(track.kind, TrackKind::Video(_)))
        .ok_or("the asset declares no video track a sprite can be cut from")?;

    let path = descriptor.track_path(track_descriptor);
    let track = Track::probe(
        op,
        &path,
        Some(track_descriptor),
        &descriptor.segment_options,
    )
    .await?;
    let sprite = Sprite {
        tile_size: args.tile_size,
        step: args.step,
        time: args.time,
    };

    op.write(&args.output, sprite.generate(op, &track).await?)
        .await?;
    println!("wrote {}", args.output);
    Ok(())
}
