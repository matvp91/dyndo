//! HLS playlist generation for dyndo assets.

mod master;
mod media;
pub mod options;
mod roles;

use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::Track;
use hls_m3u8::{MasterPlaylist, MediaPlaylist};

use options::HlsOptions;

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error(transparent)]
    Playlist(#[from] hls_m3u8::Error),
    #[error("invalid video frame rate: {0}")]
    InvalidFrameRate(String),
}

/// Generates an HLS multivariant playlist for an asset.
///
/// # Errors
///
/// Returns a [`HlsError`] when the resulting playlist is invalid.
pub fn generate_master_playlist(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MasterPlaylist<'static>, HlsError> {
    Ok(master::build_playlist(tracks, segment_options, hls_options)?.build()?)
}

/// Generates the static HLS media playlist for one asset track.
///
/// # Errors
///
/// Returns a [`HlsError`] when the resulting playlist is invalid.
pub fn generate_media_playlist(
    track: &Track,
    segment_options: &SegmentOptions,
    hls_options: &HlsOptions,
) -> Result<MediaPlaylist<'static>, HlsError> {
    Ok(media::build_playlist(track, segment_options, hls_options)?.build()?)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dyndo_core::codec::{Ac3Codec, CodecConfig};
    use dyndo_core::segment::{InitSegment, Segment};
    use dyndo_core::track_kind::{AudioKind, TrackKind, VideoKind};
    use language_tags::LanguageTag;
    use mp4_atom::{Avc1, Avcc};

    use super::{HlsOptions, SegmentOptions, Track, generate_media_playlist};

    fn audio_track() -> Track {
        let init = Arc::new(InitSegment::new(CodecConfig::Ac3(Ac3Codec), 1_000, 0, 100));
        Track::new(
            "audio-en".into(),
            "audio.mp4".into(),
            TrackKind::Audio(AudioKind {
                sample_rate: 48_000,
                channels: 2,
                language: "en".parse::<LanguageTag>().unwrap(),
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
    fn master_playlist_contains_a_video_variant() {
        let playlist = super::generate_master_playlist(
            &[video_track()],
            &SegmentOptions::default(),
            &HlsOptions::default(),
        )
        .unwrap()
        .to_string();

        assert!(playlist.contains("#EXT-X-STREAM-INF"));
        assert!(playlist.contains("video-main.m3u8"));
    }

    #[test]
    fn media_playlist_contains_the_initialization_and_media_urls() {
        let playlist = generate_media_playlist(
            &audio_track(),
            &SegmentOptions::default(),
            &HlsOptions::default(),
        )
        .unwrap()
        .to_string();

        assert!(playlist.contains("#EXT-X-MAP:URI=\"audio-en/init.mp4\""));
        assert!(playlist.contains("audio-en/0.m4s"));
    }
}
