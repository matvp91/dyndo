use dyndo_core::image::Thumbnail;
use m3u8_rs::{ExtTag, MediaPlaylist, MediaPlaylistType, MediaSegment};

pub(crate) fn stream_inf(thumbnail: &Thumbnail<'_>) -> ExtTag {
    let (width, height) = thumbnail.tile_dimensions();

    ExtTag {
        tag: "X-IMAGE-STREAM-INF".to_string(),
        rest: Some(format!(
            "BANDWIDTH={},CODECS=\"jpeg\",RESOLUTION={width}x{height},URI=\"{}.m3u8\"",
            thumbnail.bandwidth(),
            crate::image_resource_name(thumbnail),
        )),
    }
}

pub(crate) fn build_playlist(thumbnail: &Thumbnail<'_>) -> MediaPlaylist {
    let duration = u64::from(thumbnail.source().duration());
    let sprite_duration = thumbnail.sprite_duration();
    let target_duration = sprite_duration.min(duration).div_ceil(1_000);
    let (width, height) = thumbnail.tile_dimensions();
    let segments = (0..duration)
        .step_by(usize::try_from(sprite_duration).unwrap_or(usize::MAX))
        .enumerate()
        .map(|(index, start)| {
            let remaining = duration - start;
            let image_duration = remaining.min(sprite_duration);
            let mut unknown_tags = Vec::with_capacity(2);
            if index == 0 {
                unknown_tags.push(ExtTag {
                    tag: "X-IMAGES-ONLY".to_string(),
                    rest: None,
                });
            }
            unknown_tags.push(ExtTag {
                tag: "X-TILES".to_string(),
                rest: Some(format!(
                    "RESOLUTION={width}x{height},LAYOUT={}x{},DURATION={}",
                    thumbnail.tile_size(),
                    thumbnail.tile_size(),
                    seconds(u64::from(thumbnail.step())),
                )),
            });
            MediaSegment {
                uri: format!("{}/{}.jpg", crate::image_resource_name(thumbnail), start),
                duration: image_duration as f32 / 1_000.0,
                title: None,
                byte_range: None,
                discontinuity: false,
                key: None,
                map: None,
                program_date_time: None,
                daterange: None,
                unknown_tags,
            }
        })
        .collect();

    MediaPlaylist {
        version: Some(6),
        target_duration,
        media_sequence: 0,
        segments,
        discontinuity_sequence: 0,
        end_list: true,
        playlist_type: Some(MediaPlaylistType::Vod),
        i_frames_only: false,
        start: None,
        independent_segments: false,
        unknown_tags: Vec::new(),
    }
}

fn seconds(milliseconds: u64) -> String {
    format!("{}.{:03}", milliseconds / 1_000, milliseconds % 1_000)
}
