use dyndo_text::layer::WvttLayer;
use futures_util::future::try_join_all;
use opendal::Operator;

use crate::asset_descriptor::AssetDescriptor;
use crate::asset_descriptor::TrackKind;
use crate::segment::SegmentOptions;
use crate::track::{Track, TrackError};

/// Clones `op` with the layers that present a stored file as a CMAF track, packing
/// subtitle documents into `wvtt` as they are read.
pub(crate) fn add_operator_layers(op: &Operator, options: &SegmentOptions) -> Operator {
    op.clone().layer(WvttLayer::new(
        options.boundaries_ms(),
        options.text_segment_length_ms,
    ))
}

/// Reads every track declared by `asset` concurrently, packaging subtitle
/// documents as `options` describes.
///
/// Returned tracks retain descriptor order and use descriptor metadata for
/// their track kind.
///
/// # Errors
///
/// Returns the first [`TrackError`] encountered while reading the tracks.
pub async fn read_all_tracks(
    op: &Operator,
    asset: &AssetDescriptor,
    options: &SegmentOptions,
) -> Result<Vec<Track>, TrackError> {
    let reads = asset.tracks.iter().map(|descriptor| {
        let path = asset.track_path(descriptor);
        let kind = descriptor.kind.clone();
        async move { Track::probe(op, &path, Some(kind), options).await }
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
pub fn max_segment_duration_ms(tracks: &[Track], options: &SegmentOptions) -> u64 {
    tracks
        .iter()
        .filter(|track| matches!(track.kind(), TrackKind::Video(_) | TrackKind::Audio(_)))
        .flat_map(|track| {
            let timescale = track.timescale();
            track.segments(options).into_iter().map(move |segment| {
                let duration_ms =
                    (u128::from(segment.duration()) * 1000).div_ceil(u128::from(timescale));
                u64::try_from(duration_ms).unwrap_or(u64::MAX)
            })
        })
        .max()
        .unwrap_or(0)
}

/// Returns the highest average grouped-segment bitrate in bits per second.
pub fn max_bitrate(track: &Track, options: &SegmentOptions) -> u64 {
    track
        .segments(options)
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
pub fn average_bitrate(track: &Track, options: &SegmentOptions) -> u64 {
    let (byte_size, duration) =
        track
            .segments(options)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_descriptor::{AudioKind, TextKind, VideoKind};
    use crate::track::{Fragment, test_track};

    #[test]
    fn max_duration_prefers_video_over_longer_audio() {
        let tracks = vec![track(audio_kind(), 10_000), track(video_kind(), 4_000)];

        assert_eq!(max_duration_ms(&tracks), 4_000);
    }

    #[test]
    fn max_duration_falls_back_to_audio_without_video() {
        let tracks = vec![track(text_kind(), 20_000), track(audio_kind(), 5_000)];

        assert_eq!(max_duration_ms(&tracks), 5_000);
    }

    #[test]
    fn max_duration_ignores_text_only_assets() {
        let tracks = vec![track(text_kind(), 20_000)];

        assert_eq!(max_duration_ms(&tracks), 0);
    }

    #[test]
    fn max_segment_duration_excludes_text_and_rounds_up() {
        let tracks = vec![
            test_track(video_kind(), 3, vec![Fragment::new(0, 10, 1).unwrap()]),
            track(text_kind(), 10_000),
        ];

        assert_eq!(
            max_segment_duration_ms(&tracks, &SegmentOptions::default()),
            334
        );
    }

    #[test]
    fn max_bitrate_returns_highest_segment_rate() {
        let track = test_track(
            video_kind(),
            1_000,
            vec![
                Fragment::new(0, 1_000, 1_000).unwrap(),
                Fragment::new(1_000, 2_000, 1_000).unwrap(),
            ],
        );

        assert_eq!(max_bitrate(&track, &SegmentOptions::default()), 16_000);
    }

    #[test]
    fn average_bitrate_uses_all_segment_bytes_and_duration() {
        let track = test_track(
            video_kind(),
            1_000,
            vec![
                Fragment::new(0, 1_000, 1_000).unwrap(),
                Fragment::new(1_000, 2_000, 1_000).unwrap(),
            ],
        );

        assert_eq!(average_bitrate(&track, &SegmentOptions::default()), 12_000);
    }

    #[test]
    fn bitrates_are_zero_without_segments() {
        let track = test_track(video_kind(), 1_000, Vec::new());

        assert_eq!(
            (
                max_bitrate(&track, &SegmentOptions::default()),
                average_bitrate(&track, &SegmentOptions::default())
            ),
            (0, 0)
        );
    }

    fn track(kind: TrackKind, duration: u64) -> Track {
        test_track(kind, 1_000, vec![Fragment::new(0, 10, duration).unwrap()])
    }

    fn video_kind() -> TrackKind {
        TrackKind::Video(VideoKind {
            width: 1920,
            height: 1080,
            frame_rate: "25/1".to_string(),
        })
    }

    fn audio_kind() -> TrackKind {
        TrackKind::Audio(AudioKind {
            sample_rate: 48_000,
            channels: 2,
            language: "eng".parse().unwrap(),
            role: None,
        })
    }

    fn text_kind() -> TrackKind {
        TrackKind::Text(TextKind {
            language: "eng".parse().unwrap(),
            role: None,
        })
    }
}
