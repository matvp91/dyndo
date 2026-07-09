//! Deterministic, collision-proof track ids, derived from parsed fields only.

use crate::cmaf::{CmafHeader, TrackMeta};

/// Short codec family token derived from the sample-entry fourcc (also matches a
/// full RFC6381 codec string by prefix).
fn codec_token(codec: &str) -> &'static str {
    if codec.starts_with("avc1") || codec.starts_with("avc3") {
        "avc"
    } else if codec.starts_with("hvc1") || codec.starts_with("hev1") {
        "hevc"
    } else if codec.starts_with("av01") {
        "av1"
    } else if codec.starts_with("mp4a") {
        "aac"
    } else if codec.starts_with("ac-3") {
        "ac3"
    } else if codec.starts_with("ec-3") {
        "ec3"
    } else {
        "unknown"
    }
}

/// Bandwidth in kilobits per second, rounded to nearest.
fn kbps(bandwidth: u32) -> u32 {
    (bandwidth + 500) / 1000
}

/// The track's routing id, built from its whole `CmafHeader`:
/// `video_<codec>_<height>_<kbps>` or `audio_<codec>_<lang>_<channels>_<kbps>`.
pub fn track_id(header: &CmafHeader) -> String {
    let rate = kbps(header.bandwidth);
    match &header.track {
        TrackMeta::Video(m) => format!("video_{}_{}_{}", codec_token(m.fourcc), m.height, rate),
        TrackMeta::Audio(m) => format!(
            "audio_{}_{}_{}_{}",
            codec_token(m.fourcc),
            m.language.as_deref().unwrap_or("und"),
            m.channels,
            rate
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{AudioMeta, ByteRange, VideoMeta};

    fn header(track: TrackMeta, bandwidth: u32) -> CmafHeader {
        CmafHeader {
            timescale: 0,
            duration: 0,
            bandwidth,
            init_range: ByteRange { start: 0, end: 0 },
            segments: Vec::new(),
            track,
        }
    }

    fn video(fourcc: &'static str, height: u32) -> TrackMeta {
        TrackMeta::Video(VideoMeta {
            fourcc,
            width: 0,
            height,
        })
    }

    fn audio(fourcc: &'static str, language: Option<&str>, channels: u16) -> TrackMeta {
        TrackMeta::Audio(AudioMeta {
            fourcc,
            sample_rate: 0,
            channels,
            language: language.map(str::to_string),
        })
    }

    #[test]
    fn video_id_uses_codec_token_height_and_kbps() {
        assert_eq!(
            track_id(&header(video("avc1", 1080), 4807228)),
            "video_avc_1080_4807"
        );
    }

    #[test]
    fn audio_id_uses_language_channels_and_kbps() {
        assert_eq!(
            track_id(&header(audio("mp4a", Some("nld"), 2), 196918)),
            "audio_aac_nld_2_197"
        );
    }

    #[test]
    fn audio_id_defaults_absent_language_to_und() {
        assert_eq!(
            track_id(&header(audio("mp4a", None, 2), 196918)),
            "audio_aac_und_2_197"
        );
    }

    #[test]
    fn video_id_recognises_av1_codec_token() {
        assert_eq!(
            track_id(&header(video("av01", 1080), 4807228)),
            "video_av1_1080_4807"
        );
    }

    #[test]
    fn audio_id_recognises_ac3_and_ec3_codec_tokens() {
        assert_eq!(
            track_id(&header(audio("ac-3", Some("eng"), 6), 448000)),
            "audio_ac3_eng_6_448"
        );
        assert_eq!(
            track_id(&header(audio("ec-3", Some("eng"), 8), 768000)),
            "audio_ec3_eng_8_768"
        );
    }
}
