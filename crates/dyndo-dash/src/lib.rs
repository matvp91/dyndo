//! Static DASH manifest generation for dyndo assets.

mod adaptation_group;
mod builder;
mod compact;
pub mod options;
mod roles;

use dash_mpd::MPD;
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;

use options::DashOptions;

#[derive(Debug, thiserror::Error)]
pub enum DashError {
    #[error("tracks in an adaptation set are not segment-aligned")]
    SegmentAlignment,
}

/// Generates a static DASH media presentation description for an asset.
///
/// # Errors
///
/// Returns a [`DashError`] when tracks grouped into an AdaptationSet are not
/// segment-aligned.
pub fn generate_mpd(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let mut mpd = builder::build_mpd(tracks, segment_options, dash_options)?;
    if dash_options.compact {
        compact::compact(&mut mpd);
    }

    Ok(mpd)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{DashOptions, SegmentOptions, Track, generate_mpd};
    use dyndo_core::codec::{Ac3Codec, CodecConfig};
    use dyndo_core::segment::{InitSegment, Segment};
    use dyndo_core::track_kind::{AudioKind, TrackKind, VideoKind};
    use mp4_atom::{Avc1, Avcc};

    fn audio_track() -> Track {
        let init = Arc::new(InitSegment::new(CodecConfig::Ac3(Ac3Codec), 1_000, 0, 100));
        Track::new(
            "audio-en".into(),
            "audio.mp4".into(),
            TrackKind::Audio(AudioKind {
                sample_rate: 48_000,
                channels: 2,
                language: "en".parse().unwrap(),
                role: None,
            }),
            Arc::clone(&init),
            vec![Segment::new(init, 0, 1_000, 100, 200)],
        )
    }

    fn video_track() -> Track {
        let codec = CodecConfig::Avc(dyndo_core::codec::AvcCodec::new(&Avc1 {
            avcc: Avcc {
                avc_profile_indication: 0x42,
                avc_level_indication: 0x1e,
                length_size: 4,
                ..Avcc::default()
            },
            ..Avc1::default()
        }));
        let init = Arc::new(InitSegment::new(codec, 1_000, 0, 100));
        Track::new(
            "video-main".into(),
            "video.mp4".into(),
            TrackKind::Video(VideoKind {
                width: 16,
                height: 16,
                frame_rate: "4/1".into(),
            }),
            Arc::clone(&init),
            vec![Segment::new(init, 0, 1_000, 100, 200)],
        )
    }

    #[test]
    fn mpd_contains_the_video_representation_and_frame_rate() {
        let mpd = generate_mpd(
            &[video_track()],
            &SegmentOptions::default(),
            &DashOptions::default(),
        )
        .unwrap();
        let representation = &mpd.periods[0].adaptations[0].representations[0];

        assert_eq!(representation.id.as_deref(), Some("video-main"));
        assert_eq!(representation.frameRate.as_deref(), Some("4/1"));
    }

    #[test]
    fn mpd_contains_the_audio_representation_and_segment_template() {
        let mpd = generate_mpd(
            &[audio_track()],
            &SegmentOptions::default(),
            &DashOptions::default(),
        )
        .unwrap();

        assert_eq!(
            mpd.periods[0].adaptations[0].representations[0]
                .id
                .as_deref(),
            Some("audio-en")
        );
        assert_eq!(
            mpd.periods[0].adaptations[0].representations[0]
                .SegmentTemplate
                .as_ref()
                .unwrap()
                .media
                .as_deref(),
            Some("$RepresentationID$/$Time$.m4s")
        );
    }
}
