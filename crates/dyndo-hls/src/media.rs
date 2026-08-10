use std::time::Duration;

use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;
use hls_m3u8::builder::MediaPlaylistBuilder;
use hls_m3u8::tags::ExtXMap;
use hls_m3u8::types::PlaylistType;
use hls_m3u8::{MediaPlaylist, MediaSegment};

use crate::HlsError;
use crate::options::HlsOptions;

pub(crate) fn build_playlist(
    track: &Track,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MediaPlaylistBuilder<'static>, HlsError> {
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
    let segments = build_segments(track, &segments, plain_vtt)?;

    let mut builder = MediaPlaylist::builder();
    builder
        .target_duration(Duration::from_secs(target_duration))
        .playlist_type(PlaylistType::Vod)
        .has_end_list(true)
        .segments(segments);
    Ok(builder)
}

fn build_segments(
    track: &Track,
    segments: &[ServedSegment<'_>],
    plain_vtt: bool,
) -> Result<Vec<MediaSegment<'static>>, HlsError> {
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
) -> Result<MediaSegment<'static>, HlsError> {
    let extension = if plain_vtt { "vtt" } else { "m4s" };
    let start_time = segment.unscaled_start_time();
    let mut builder = MediaSegment::builder();
    builder
        .duration(media_duration(
            segment.unscaled_duration(),
            track.timescale(),
        ))
        .uri(format!("{}/{start_time}.{extension}", track.id()));
    if first && !plain_vtt {
        builder.map(ExtXMap::new(format!("{}/init.mp4", track.id())));
    }

    Ok(builder.build()?)
}

fn media_duration(unscaled_duration: u64, timescale: u32) -> Duration {
    let duration =
        (u128::from(unscaled_duration) * 1_000 + u128::from(timescale) / 2) / u128::from(timescale);
    Duration::from_millis(u64::try_from(duration).unwrap_or(u64::MAX))
}

fn rounded_duration_seconds(unscaled_duration: u64, timescale: u32) -> u64 {
    let duration = u128::from(unscaled_duration);
    let timescale = u128::from(timescale);
    u64::try_from((duration + timescale / 2) / timescale).unwrap_or(u64::MAX)
}
