use clap::Args;
use dyndo_core::asset::AssetDescriptor;
use dyndo_core::image::FrameExtractor;
use dyndo_core::track::SourceTrack;
use dyndo_core::track::kind::CmafTrackKind;
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
        .source_tracks()
        .find(|track| matches!(track.cmaf_kind(), Some(CmafTrackKind::Video(_))))
        .ok_or("asset has no video track")?;
    let path = asset.track_path(descriptor);
    let track = SourceTrack::probe(op, &path, Some(descriptor)).await?;
    let cmaf = track.cmaf().ok_or("video track is not CMAF")?;
    let CmafTrackKind::Video(video) = cmaf.kind() else {
        return Err("probed track is not a video track".into());
    };
    let jpeg = FrameExtractor::new(op, cmaf)
        .jpeg(args.time, video.width, video.height)
        .await?;

    op.write(&args.output, jpeg).await?;
    println!("wrote {}", args.output);
    Ok(())
}
