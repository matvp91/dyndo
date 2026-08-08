use clap::Args;
use dyndo_core::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
use dyndo_core::track::Track;
use dyndo_image::sprite_generator;
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
    /// Height of the whole sprite, in pixels. A thumbnail is a `--tile-size`th of it,
    /// and its width follows the source's aspect.
    #[arg(long)]
    height: u32,
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
    let track_descriptor = best_video_track(&descriptor, args.height / args.tile_size)
        .ok_or("the asset declares no video track a sprite can be cut from")?;

    let path = descriptor.track_path(track_descriptor);
    let track = Track::probe(
        op,
        &path,
        Some(track_descriptor),
        &descriptor.segment_options,
    )
    .await?;
    let sprite = sprite_generator::generate(
        op,
        &track,
        args.tile_size,
        args.height,
        args.step,
        args.time,
    )
    .await?;

    op.write(&args.output, sprite).await?;
    println!("wrote {}", args.output);
    Ok(())
}

/// The video track a sprite is best cut from.
///
/// The shortest video track still at least as tall as a thumbnail is the cheapest to
/// decode: a taller one spends more time and more bytes on detail the downscale throws
/// away, and a shorter one would be scaled up. When every video track is shorter than a
/// thumbnail the tallest is the closest there is.
///
/// Whether the track can be decoded is left to the decoder, which refuses a codec it
/// does not handle by name.
fn best_video_track(asset: &AssetDescriptor, cell_height: u32) -> Option<&TrackDescriptor> {
    let video_tracks: Vec<(&TrackDescriptor, u32)> = asset
        .tracks
        .iter()
        .filter_map(|track| match &track.kind {
            TrackKind::Video(video) => Some((track, video.height)),
            _ => None,
        })
        .collect();

    video_tracks
        .iter()
        .filter(|(_, height)| *height >= cell_height)
        .min_by_key(|(_, height)| *height)
        .or_else(|| video_tracks.iter().max_by_key(|(_, height)| *height))
        .map(|(track, _)| *track)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_video_track_prefers_the_shortest_video_track_tall_enough_for_a_thumbnail() {
        let asset = asset(&[
            ("low", "avc1.42c00d", 320),
            ("mid", "avc1.4d401f", 1280),
            ("high", "avc1.640028", 1920),
        ]);

        assert_eq!(
            best_video_track(&asset, 360).map(|track| track.id.as_str()),
            Some("mid")
        );
    }

    #[test]
    fn best_video_track_falls_back_to_the_tallest_video_track_available() {
        let asset = asset(&[("low", "avc1.42c00d", 160), ("mid", "avc1.4d401f", 320)]);

        assert_eq!(
            best_video_track(&asset, 360).map(|track| track.id.as_str()),
            Some("mid")
        );
    }

    /// Codec is the decoder's to judge, so a video track it cannot handle is still the
    /// one chosen on size and is refused later by name.
    #[test]
    fn best_video_track_leaves_the_codec_to_the_decoder() {
        let asset = asset(&[("av1", "av01.0.05M.08", 1920)]);

        assert_eq!(
            best_video_track(&asset, 180).map(|track| track.id.as_str()),
            Some("av1")
        );
    }

    fn asset(video_tracks: &[(&str, &str, u32)]) -> AssetDescriptor {
        let json = video_tracks
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
}
