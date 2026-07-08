//! RFC6381 codec-string assembly from decoder-config fields.

/// `avc1.PPCCLL` from AVCDecoderConfigurationRecord profile/compat/level bytes.
pub fn avc_codec_string(profile: u8, compat: u8, level: u8) -> String {
    format!("avc1.{:02x}{:02x}{:02x}", profile, compat, level)
}

/// `mp4a.40.<audio_object_type>` (0x40 = MPEG-4 Audio object-type-indication).
pub fn aac_codec_string(audio_object_type: u8) -> String {
    format!("mp4a.40.{}", audio_object_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avc_high_profile_level_4() {
        // profile 0x64, compat 0x00, level 0x28 -> avc1.640028
        assert_eq!(avc_codec_string(0x64, 0x00, 0x28), "avc1.640028");
    }

    #[test]
    fn aac_lc() {
        assert_eq!(aac_codec_string(2), "mp4a.40.2");
    }
}
