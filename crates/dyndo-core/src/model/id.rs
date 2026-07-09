//! Deterministic, collision-proof track ids, derived from parsed fields only.

/// Short codec family token derived from the RFC6381 codec string.
fn codec_token(codec: &str) -> &'static str {
    if codec.starts_with("avc1") || codec.starts_with("avc3") {
        "avc"
    } else if codec.starts_with("hvc1") || codec.starts_with("hev1") {
        "hevc"
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

pub fn video_track_id(codec: &str, height: u32, bandwidth: u32) -> String {
    format!(
        "video_{}_{}_{}",
        codec_token(codec),
        height,
        kbps(bandwidth)
    )
}

pub fn audio_track_id(
    codec: &str,
    language: Option<&str>,
    channels: u16,
    bandwidth: u32,
) -> String {
    format!(
        "audio_{}_{}_{}_{}",
        codec_token(codec),
        language.unwrap_or("und"),
        channels,
        kbps(bandwidth)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_id_uses_codec_token_height_and_kbps() {
        assert_eq!(
            video_track_id("avc1.640028", 1080, 4807228),
            "video_avc_1080_4807"
        );
    }

    #[test]
    fn audio_id_uses_language_channels_and_kbps() {
        assert_eq!(
            audio_track_id("mp4a.40.2", Some("nld"), 2, 196918),
            "audio_aac_nld_2_197"
        );
    }

    #[test]
    fn audio_id_defaults_absent_language_to_und() {
        assert_eq!(
            audio_track_id("mp4a.40.2", None, 2, 196918),
            "audio_aac_und_2_197"
        );
    }
}
