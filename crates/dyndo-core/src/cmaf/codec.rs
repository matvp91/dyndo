//! Codec identity: the parsed codec-param enums, their fourcc, and RFC 6381
//! codec-string assembly. mp4-atom sample entries are projected into these enums
//! by `video_codec`/`audio_codec` (added in a later task); nothing here escapes cmaf.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoCodec {
    Avc {
        profile: u8,
        constraints: u8,
        level: u8,
    },
    Av1 {
        seq_profile: u8,
        seq_level_idx: u8,
        tier: bool,
        high_bitdepth: bool,
        twelve_bit: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCodec {
    Aac { audio_object_type: u8 },
    Ac3,
    Ec3,
}

impl VideoCodec {
    pub fn fourcc(&self) -> &'static str {
        match self {
            VideoCodec::Avc { .. } => "avc1",
            VideoCodec::Av1 { .. } => "av01",
        }
    }

    /// RFC 6381 codecs parameter.
    pub fn rfc6381(&self) -> String {
        match *self {
            VideoCodec::Avc {
                profile,
                constraints,
                level,
            } => {
                format!("avc1.{profile:02x}{constraints:02x}{level:02x}")
            }
            VideoCodec::Av1 {
                seq_profile,
                seq_level_idx,
                tier,
                high_bitdepth,
                twelve_bit,
            } => {
                let t = if tier { 'H' } else { 'M' };
                let bit_depth = if twelve_bit {
                    12
                } else if high_bitdepth {
                    10
                } else {
                    8
                };
                format!("av01.{seq_profile}.{seq_level_idx:02}{t}.{bit_depth:02}")
            }
        }
    }
}

impl AudioCodec {
    pub fn fourcc(&self) -> &'static str {
        match self {
            AudioCodec::Aac { .. } => "mp4a",
            AudioCodec::Ac3 => "ac-3",
            AudioCodec::Ec3 => "ec-3",
        }
    }

    /// RFC 6381 codecs parameter. AAC's object-type-indication is always 0x40 (MPEG-4 Audio).
    pub fn rfc6381(&self) -> String {
        match self {
            AudioCodec::Aac { audio_object_type } => format!("mp4a.40.{audio_object_type}"),
            AudioCodec::Ac3 => "ac-3".to_string(),
            AudioCodec::Ec3 => "ec-3".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_fourcc() {
        assert_eq!(
            VideoCodec::Avc {
                profile: 0x64,
                constraints: 0,
                level: 0x28
            }
            .fourcc(),
            "avc1"
        );
        assert_eq!(
            VideoCodec::Av1 {
                seq_profile: 0,
                seq_level_idx: 5,
                tier: false,
                high_bitdepth: false,
                twelve_bit: false
            }
            .fourcc(),
            "av01"
        );
    }

    #[test]
    fn audio_fourcc() {
        assert_eq!(
            AudioCodec::Aac {
                audio_object_type: 2
            }
            .fourcc(),
            "mp4a"
        );
        assert_eq!(AudioCodec::Ac3.fourcc(), "ac-3");
        assert_eq!(AudioCodec::Ec3.fourcc(), "ec-3");
    }

    #[test]
    fn avc_rfc6381() {
        assert_eq!(
            VideoCodec::Avc {
                profile: 0x64,
                constraints: 0x00,
                level: 0x28
            }
            .rfc6381(),
            "avc1.640028"
        );
    }

    #[test]
    fn av1_rfc6381() {
        assert_eq!(
            VideoCodec::Av1 {
                seq_profile: 0,
                seq_level_idx: 5,
                tier: false,
                high_bitdepth: false,
                twelve_bit: false
            }
            .rfc6381(),
            "av01.0.05M.08"
        );
        assert_eq!(
            VideoCodec::Av1 {
                seq_profile: 0,
                seq_level_idx: 9,
                tier: true,
                high_bitdepth: true,
                twelve_bit: false
            }
            .rfc6381(),
            "av01.0.09H.10"
        );
    }

    #[test]
    fn audio_rfc6381() {
        assert_eq!(
            AudioCodec::Aac {
                audio_object_type: 2
            }
            .rfc6381(),
            "mp4a.40.2"
        );
        assert_eq!(AudioCodec::Ac3.rfc6381(), "ac-3");
        assert_eq!(AudioCodec::Ec3.rfc6381(), "ec-3");
    }
}
