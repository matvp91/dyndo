use std::collections::HashMap;

use dyndo_core::time::Time;
use dyndo_core::track::cmaf::{ResolvedCmafTrack, ServedSegment};
use dyndo_core::track::thumbnail::ResolvedThumbnailTrack;
use m3u8_rs::{ExtTag, Map, MediaPlaylist, MediaPlaylistType, MediaSegment};

pub(crate) fn build_playlist(
    track: &ResolvedCmafTrack,
    min_length: u32,
    boundaries: &[u32],
    plain_vtt: bool,
) -> MediaPlaylist {
    let segments = track.served_segments(min_length, boundaries);
    let target_duration = segments
        .iter()
        .map(|segment| rounded_duration_seconds(segment.unscaled_duration(), track.timescale()))
        .max()
        .unwrap_or(0);
    let segments = build_segments(track, &segments, plain_vtt);

    MediaPlaylist {
        version: (!plain_vtt).then_some(6),
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

pub(crate) fn build_image_playlist(thumbnail: &ResolvedThumbnailTrack) -> MediaPlaylist {
    let duration = u64::from(thumbnail.source().duration());
    let sprite_duration = thumbnail.sprite_duration();
    let target_duration = sprite_duration.min(duration).div_ceil(1_000);
    let (width, height) = thumbnail.tile_dimensions();
    let segments = (1..=duration.div_ceil(sprite_duration.max(1)))
        .map(|number| {
            let start = (number - 1).saturating_mul(sprite_duration);
            let remaining = duration - start;
            let image_duration = remaining.min(sprite_duration);
            let mut unknown_tags = Vec::with_capacity(2);
            if number == 1 {
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
                    seconds(thumbnail.frame_duration()),
                )),
            });
            MediaSegment {
                uri: format!("{}/{number}.jpg", thumbnail.id()),
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

fn build_segments(
    track: &ResolvedCmafTrack,
    segments: &[ServedSegment<'_>],
    plain_vtt: bool,
) -> Vec<MediaSegment> {
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| build_segment(track, segment, index == 0, plain_vtt))
        .collect()
}

fn build_segment(
    track: &ResolvedCmafTrack,
    segment: &ServedSegment<'_>,
    first: bool,
    plain_vtt: bool,
) -> MediaSegment {
    let extension = if plain_vtt { "vtt" } else { "m4s" };
    let start_time = segment.unscaled_start_time();
    let keys = if first {
        crate::protection::keys(track.protection())
    } else {
        Vec::new()
    };
    let mut keys = keys.into_iter();
    let key = keys.next();
    let unknown_tags = keys.map(crate::protection::key_tag).collect();
    MediaSegment {
        uri: format!("{}/{start_time}.{extension}", track.id()),
        duration: media_duration(segment.unscaled_duration(), track.timescale()),
        title: None,
        byte_range: None,
        discontinuity: false,
        key,
        map: (first && !plain_vtt).then(|| Map {
            uri: format!("{}/init.mp4", track.id()),
            byte_range: None,
            other_attributes: HashMap::new(),
        }),
        program_date_time: None,
        daterange: None,
        unknown_tags,
    }
}

fn media_duration(unscaled_duration: u64, timescale: u32) -> f32 {
    Time::milliseconds_rounded(unscaled_duration, timescale) as f32 / 1_000.0
}

fn rounded_duration_seconds(unscaled_duration: u64, timescale: u32) -> u64 {
    Time::seconds_rounded(unscaled_duration, timescale)
}

fn seconds(milliseconds: u64) -> String {
    format!("{}.{:03}", milliseconds / 1_000, milliseconds % 1_000)
}
