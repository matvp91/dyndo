use std::collections::HashMap;

use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::cmaf::CmafTrack;
use dyndo_core::track::cmaf::kind::CmafTrackKind;
use dyndo_core::track::thumbnail::ThumbnailTrack;
use m3u8_rs::{ExtTag, Map, MediaPlaylist, MediaPlaylistType, MediaSegment};

use crate::options::HlsOptions;

pub(crate) fn build_playlist(
    track: &CmafTrack,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> MediaPlaylist {
    let plain_vtt = !hls_options.wvtt && matches!(track.kind(), CmafTrackKind::Text(_));
    let segments = ServedSegment::group(
        track.segments(),
        segment_options.min_length,
        &segment_options.boundaries,
    );
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

pub(crate) fn build_image_playlist(thumbnail: &ThumbnailTrack) -> MediaPlaylist {
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

fn build_segments(
    track: &CmafTrack,
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
    track: &CmafTrack,
    segment: &ServedSegment<'_>,
    first: bool,
    plain_vtt: bool,
) -> MediaSegment {
    let extension = if plain_vtt { "vtt" } else { "m4s" };
    let start_time = segment.unscaled_start_time();
    MediaSegment {
        uri: format!(
            "{}/{start_time}.{extension}",
            crate::media_resource_name(track)
        ),
        duration: media_duration(segment.unscaled_duration(), track.timescale()),
        title: None,
        byte_range: None,
        discontinuity: false,
        key: None,
        map: (first && !plain_vtt).then(|| Map {
            uri: format!("{}/init.mp4", crate::media_resource_name(track)),
            byte_range: None,
            other_attributes: HashMap::new(),
        }),
        program_date_time: None,
        daterange: None,
        unknown_tags: Vec::new(),
    }
}

fn media_duration(unscaled_duration: u64, timescale: u32) -> f32 {
    let duration =
        (u128::from(unscaled_duration) * 1_000 + u128::from(timescale) / 2) / u128::from(timescale);
    u64::try_from(duration).unwrap_or(u64::MAX) as f32 / 1_000.0
}

fn rounded_duration_seconds(unscaled_duration: u64, timescale: u32) -> u64 {
    let duration = u128::from(unscaled_duration);
    let timescale = u128::from(timescale);
    u64::try_from((duration + timescale / 2) / timescale).unwrap_or(u64::MAX)
}

fn seconds(milliseconds: u64) -> String {
    format!("{}.{:03}", milliseconds / 1_000, milliseconds % 1_000)
}
