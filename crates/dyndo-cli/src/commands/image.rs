use clap::Args;
use dyndo_core::asset::Asset;
use dyndo_core::image::FrameExtractor;
use dyndo_core::track::cmaf::CmafKind;
use opendal::Operator;

#[derive(Args)]
pub(crate) struct ImageArgs {
    /// Asset path.
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
    let asset = Asset::read(op, &args.input).await?;
    let source = asset
        .source_tracks()
        .find(|track| matches!(track.cmaf_kind(), Some(CmafKind::Video(_))))
        .ok_or("asset has no video track")?;
    let path = asset.track_path(source);
    let track = source.resolve(op, &path).await?;
    let cmaf = track.cmaf().ok_or("video track is not CMAF")?;
    let CmafKind::Video(video) = cmaf.kind() else {
        return Err("resolved track is not a video track".into());
    };
    let jpeg = FrameExtractor::new(op, cmaf)
        .jpeg(args.time, video.width, video.height)
        .await?;

    op.write(&args.output, jpeg).await?;
    println!("wrote {}", args.output);
    Ok(())
}
