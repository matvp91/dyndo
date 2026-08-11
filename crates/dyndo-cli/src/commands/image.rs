use clap::Args;
use dyndo_core::asset::Asset;
use dyndo_core::image::FrameExtractor;
use dyndo_core::track::SourceTrack;
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
    let source = highest_video_source(&asset).ok_or("asset has no video track")?;
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

fn highest_video_source(asset: &Asset) -> Option<&SourceTrack> {
    asset
        .source_tracks()
        .filter_map(|track| track.video_metadata().map(|video| (track, video)))
        .max_by_key(|(_, video)| {
            (
                u64::from(video.width) * u64::from(video.height),
                video.width,
                video.height,
            )
        })
        .map(|(track, _)| track)
}

#[cfg(test)]
mod tests {
    use dyndo_core::asset::Asset;

    use super::highest_video_source;

    #[test]
    fn highest_video_source_selects_the_largest_rendition() {
        let asset: Asset = serde_json::from_str(
            r#"{
                "tracks": [
                    {
                        "id": "720p",
                        "path": "video_720.mp4",
                        "codec": "avc1.64001f",
                        "type": "video",
                        "width": 1280,
                        "height": 720,
                        "frame_rate": "25/1"
                    },
                    {
                        "id": "1080p",
                        "path": "video_1080.mp4",
                        "codec": "avc1.640028",
                        "type": "video",
                        "width": 1920,
                        "height": 1080,
                        "frame_rate": "25/1"
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(highest_video_source(&asset).unwrap().id(), "1080p");
    }
}
