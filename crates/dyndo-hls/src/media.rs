use std::collections::HashMap;

use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;
use m3u8_rs::{Map, MediaPlaylist, MediaPlaylistType, MediaSegment};

use crate::options::HlsOptions;

pub(crate) fn build_playlist(
    track: &Track,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> MediaPlaylist {
    let plain_vtt = !hls_options.wvtt && matches!(track.kind(), TrackKind::Text(_));
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

fn build_segments(
    track: &Track,
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
    track: &Track,
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
