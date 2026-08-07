use clap::Args;
use dyndo_core::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
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
    /// Thumbnails per sheet row, and per sheet column.
    #[arg(long)]
    grid: u32,
    /// Width one thumbnail is scaled to, in pixels. Its height follows the source's
    /// aspect.
    #[arg(long = "cell-width")]
    cell_width: u32,
    /// Milliseconds between one thumbnail and the next.
    #[arg(long)]
    cadence: u32,
    /// Presentation time the sheet's first thumbnail shows, in milliseconds. Each
    /// thumbnail after it steps on by the cadence.
    #[arg(long, default_value_t = 0)]
    time: u64,
}

pub(super) async fn run(op: &Operator, args: SpriteArgs) -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = AssetDescriptor::read(op, &args.input).await?;
    let sprite = Sprite {
        grid: args.grid,
        cell_width: args.cell_width,
        cadence: args.cadence,
    };
    let track_descriptor = best_video_track(&descriptor, args.cell_width)
        .ok_or("the asset declares no video track a sheet can be cut from")?;

    let path = descriptor.track_path(track_descriptor);
    let segment_options = &descriptor.segment_options;
    let track = Track::probe(
        op,
        &path,
        Some(track_descriptor.kind.clone()),
        segment_options,
    )
    .await?;
    let sheet = sprite
        .generate(op, &track, segment_options, args.time)
        .await?;

    let (width, height) = sprite.size(source_size(track_descriptor));
    op.write(&args.output, sheet).await?;
    println!(
        "wrote {} ({width}x{height}, {} thumbnails from {})",
        args.output,
        args.grid * args.grid,
        track_descriptor.id
    );
    Ok(())
}

/// The video track a sheet is best cut from.
///
/// The narrowest rendition still at least as wide as a thumbnail is the cheapest to
/// decode: a wider one spends more time and more bytes on detail the downscale throws
/// away, and a narrower one would be scaled up. When every rendition is narrower than
/// a thumbnail the widest is the closest there is.
///
/// Whether the rendition can be decoded is left to the decoder, which refuses a codec
/// it does not handle by name.
fn best_video_track(asset: &AssetDescriptor, cell_width: u32) -> Option<&TrackDescriptor> {
    let renditions: Vec<(&TrackDescriptor, u32)> = asset
        .tracks
        .iter()
        .filter_map(|track| match &track.kind {
            TrackKind::Video(video) => Some((track, video.width)),
            _ => None,
        })
        .collect();

    renditions
        .iter()
        .filter(|(_, width)| *width >= cell_width)
        .min_by_key(|(_, width)| *width)
        .or_else(|| renditions.iter().max_by_key(|(_, width)| *width))
        .map(|(track, _)| *track)
}

/// The pixel size of the track a sheet's thumbnails are scaled down from.
fn source_size(track: &TrackDescriptor) -> (u32, u32) {
    match &track.kind {
        TrackKind::Video(video) => (video.width, video.height),
        _ => unreachable!("only a video track is ever chosen"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_video_track_prefers_the_narrowest_rendition_wide_enough_for_a_thumbnail() {
        let asset = asset(&[
            ("low", "avc1.42c00d", 320),
            ("mid", "avc1.4d401f", 1280),
            ("high", "avc1.640028", 1920),
        ]);

        assert_eq!(
            best_video_track(&asset, 640).map(|track| track.id.as_str()),
            Some("mid")
        );
    }

    #[test]
    fn best_video_track_falls_back_to_the_widest_rendition_available() {
        let asset = asset(&[("low", "avc1.42c00d", 160), ("mid", "avc1.4d401f", 320)]);

        assert_eq!(
            best_video_track(&asset, 640).map(|track| track.id.as_str()),
            Some("mid")
        );
    }

    /// Codec is the decoder's to judge, so a rendition it cannot handle is still the
    /// one chosen on size and is refused later by name.
    #[test]
    fn best_video_track_leaves_the_codec_to_the_decoder() {
        let asset = asset(&[("av1", "av01.0.05M.08", 1920)]);

        assert_eq!(
            best_video_track(&asset, 320).map(|track| track.id.as_str()),
            Some("av1")
        );
    }

    fn asset(renditions: &[(&str, &str, u32)]) -> AssetDescriptor {
        let json = renditions
            .iter()
            .map(|(id, codec, width)| {
                format!(
                    r#"{{"id":"{id}","path":"{id}.mp4","codec":"{codec}","type":"video",
                       "width":{width},"height":{},"frame_rate":"25/1"}}"#,
                    width * 9 / 16
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        serde_json::from_str(&format!(r#"{{"tracks":[{json}]}}"#)).unwrap()
    }

    #[test]
    fn source_size_reads_the_chosen_rendition() {
        let asset = asset(&[("main", "avc1.640028", 1920)]);

        assert_eq!(source_size(&asset.tracks[0]), (1920, 1080));
    }
}
