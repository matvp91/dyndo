use crate::cmaf::{CmafHeader, Stream};

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

pub fn track_id(header: &CmafHeader) -> String {
    let codec = codec_token(header.stream.fourcc());
    let bitrate = kbps(header.bandwidth);
    match &header.stream {
        Stream::Video(v) => format!("video_{}_{}_{}", codec, v.height, bitrate),
        Stream::Audio(a) => format!(
            "audio_{}_{}_{}_{}",
            codec,
            a.language.as_deref().unwrap_or("und"),
            a.channels,
            bitrate
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{AudioCodec, AudioStream, Segment, VideoCodec, VideoStream};

    fn video_header(codec: VideoCodec, height: u32, bandwidth: u32) -> CmafHeader {
        CmafHeader {
            timescale: 0,
            duration: 0,
            bandwidth,
            earliest_presentation_time: 0,
            init_segment: Segment { offset: 0, size: 0, duration: 0 },
            segments: Vec::new(),
            stream: Stream::Video(VideoStream {
                codec,
                width: 0,
                height,
                frame_rate: (0, 1),
            }),
        }
    }

    fn audio_header(
        codec: AudioCodec,
        language: Option<&str>,
        channels: u16,
        bandwidth: u32,
    ) -> CmafHeader {
        CmafHeader {
            timescale: 0,
            duration: 0,
            bandwidth,
            earliest_presentation_time: 0,
            init_segment: Segment { offset: 0, size: 0, duration: 0 },
            segments: Vec::new(),
            stream: Stream::Audio(AudioStream {
                codec,
                sample_rate: 0,
                channels,
                language: language.map(str::to_string),
            }),
        }
    }

    fn avc() -> VideoCodec {
        VideoCodec::Avc {
            profile: 0,
            constraints: 0,
            level: 0,
        }
    }

    fn av1() -> VideoCodec {
        VideoCodec::Av1 {
            seq_profile: 0,
            seq_level_idx: 0,
            tier: false,
            high_bitdepth: false,
            twelve_bit: false,
        }
    }

    fn aac() -> AudioCodec {
        AudioCodec::Aac {
            audio_object_type: 2,
        }
    }

    #[test]
    fn video_id_uses_codec_token_height_and_kbps() {
        assert_eq!(
            track_id(&video_header(avc(), 1080, 4807228)),
            "video_avc_1080_4807"
        );
    }

    #[test]
    fn audio_id_uses_language_channels_and_kbps() {
        assert_eq!(
            track_id(&audio_header(aac(), Some("nld"), 2, 196918)),
            "audio_aac_nld_2_197"
        );
    }

    #[test]
    fn audio_id_defaults_absent_language_to_und() {
        assert_eq!(
            track_id(&audio_header(aac(), None, 2, 196918)),
            "audio_aac_und_2_197"
        );
    }

    #[test]
    fn video_id_recognises_av1_codec_token() {
        assert_eq!(
            track_id(&video_header(av1(), 1080, 4807228)),
            "video_av1_1080_4807"
        );
    }

    #[test]
    fn audio_id_recognises_ac3_and_ec3_codec_tokens() {
        assert_eq!(
            track_id(&audio_header(AudioCodec::Ac3, Some("eng"), 6, 448000)),
            "audio_ac3_eng_6_448"
        );
        assert_eq!(
            track_id(&audio_header(AudioCodec::Ec3, Some("eng"), 8, 768000)),
            "audio_ec3_eng_8_768"
        );
    }
}
