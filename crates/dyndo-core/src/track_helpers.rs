use futures_util::future::try_join_all;
use opendal::Operator;

use crate::asset_descriptor::AssetDescriptor;
use crate::asset_descriptor::TrackKind;
use crate::track::{Track, TrackError};

/// Reads and probes every track declared by `asset` concurrently.
///
/// Returned tracks retain descriptor order and use descriptor metadata for
/// their track kind.
///
/// # Errors
///
/// Returns the first [`TrackError`] encountered while probing the tracks.
pub async fn read_all_tracks(
    op: &Operator,
    asset: &AssetDescriptor,
) -> Result<Vec<Track>, TrackError> {
    let reads = asset.tracks.iter().map(|descriptor| {
        let path = asset.track_path(descriptor);
        let kind = descriptor.kind.clone();
        async move { Track::probe(op, &path, Some(kind)).await }
    });

    try_join_all(reads).await
}

/// Returns the longest video duration, or the longest audio duration when no
/// video track is present. Text tracks do not determine presentation length.
pub fn max_duration_ms(tracks: &[Track]) -> u64 {
    max_matching_duration_ms(tracks, |kind| matches!(kind, TrackKind::Video(_))).unwrap_or_else(
        || {
            max_matching_duration_ms(tracks, |kind| matches!(kind, TrackKind::Audio(_)))
                .unwrap_or(0)
        },
    )
}

/// Returns the longest audio or video segment duration in milliseconds.
pub fn max_segment_duration_ms(
    tracks: &[Track],
    boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> u64 {
    tracks
        .iter()
        .filter(|track| matches!(track.kind(), TrackKind::Video(_) | TrackKind::Audio(_)))
        .flat_map(|track| {
            let timescale = track.timescale();
            track
                .segments(boundaries_ms, min_segment_length_ms)
                .into_iter()
                .map(move |segment| {
                    let duration_ms =
                        (u128::from(segment.duration()) * 1000).div_ceil(u128::from(timescale));
                    u64::try_from(duration_ms).unwrap_or(u64::MAX)
                })
        })
        .max()
        .unwrap_or(0)
}

/// Returns the highest average grouped-segment bitrate in bits per second.
pub fn max_bitrate(track: &Track, boundaries_ms: &[u64], min_segment_length_ms: u64) -> u64 {
    track
        .segments(boundaries_ms, min_segment_length_ms)
        .iter()
        .map(|segment| {
            let bits = u128::from(segment.byte_size()) * 8;
            let scaled_bits = bits * u128::from(track.timescale());
            let bitrate = scaled_bits.div_ceil(u128::from(segment.duration()));
            u64::try_from(bitrate).unwrap_or(u64::MAX)
        })
        .max()
        .unwrap_or(0)
}

/// Returns the average bitrate of all grouped segments in bits per second.
pub fn average_bitrate(track: &Track, boundaries_ms: &[u64], min_segment_length_ms: u64) -> u64 {
    let (byte_size, duration) = track
        .segments(boundaries_ms, min_segment_length_ms)
        .iter()
        .fold((0_u128, 0_u128), |(byte_size, duration), segment| {
            (
                byte_size + u128::from(segment.byte_size()),
                duration + u128::from(segment.duration()),
            )
        });
    if duration == 0 {
        return 0;
    }

    let bits = byte_size * 8;
    let scaled_bits = bits * u128::from(track.timescale());
    u64::try_from(scaled_bits.div_ceil(duration)).unwrap_or(u64::MAX)
}

fn max_matching_duration_ms(tracks: &[Track], include: impl Fn(&TrackKind) -> bool) -> Option<u64> {
    tracks
        .iter()
        .filter(|track| include(track.kind()))
        .map(Track::duration_ms)
        .max()
}
