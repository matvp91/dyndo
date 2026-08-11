use clap::Args;
use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::image::FrameExtractor;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;
use opendal::Operator;

#[derive(Args)]
pub(crate) struct ImageArgs {
    /// Asset descriptor path.
    #[arg(short, long = "input")]
    input: String,
    /// Frame time in milliseconds.
    #[arg(short, long = "time")]
    time: u64,
    /// Output JPEG path.
    #[arg(short, long = "output")]
    output: String,
}

pub(crate) async fn run(op: &Operator, args: ImageArgs) -> Result<(), Box<dyn std::error::Error>> {
    let asset = AssetDescriptor::read(op, &args.input).await?;
    let descriptor = asset
        .tracks
        .iter()
        .find(|track| matches!(track.kind, TrackKind::Video(_)))
        .ok_or("asset has no video track")?;
    let path = asset.track_path(descriptor);
    let track = Track::probe(op, &path, Some(descriptor), &asset.segment_options).await?;
    let TrackKind::Video(video) = track.kind() else {
        return Err("probed track is not a video track".into());
    };
    let jpeg = FrameExtractor::new(op, &track)
        .jpeg(args.time, video.width, video.height)
        .await?;

    op.write(&args.output, jpeg).await?;
    println!("wrote {}", args.output);
    Ok(())
}
