//! Supported codecs and the parameters needed for their RFC 6381 strings.

use mp4_atom::{Audio, Codec, Visual};

use crate::CoreError;

/// A supported video codec with the parameters needed for its RFC 6381 string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    /// H.264/AVC, with the profile/constraints/level for its `avc1.PPCCLL` string.
    Avc {
        /// AVC profile indication byte.
        profile: u8,
        /// Profile-compatibility (constraint flags) byte.
        constraints: u8,
        /// AVC level indication byte.
        level: u8,
    },
    /// AV1, with the sequence parameters for its `av01.…` string.
    Av1 {
        /// `seq_profile` from the AV1 config.
        seq_profile: u8,
        /// `seq_level_idx_0` from the AV1 config.
        seq_level_idx: u8,
        /// Tier: `true` = high (`H`), `false` = main (`M`).
        tier: bool,
        /// Whether the stream is >8-bit.
        high_bitdepth: bool,
        /// Whether the stream is 12-bit (takes precedence over `high_bitdepth`).
        twelve_bit: bool,
    },
}

/// A supported audio codec with the parameters needed for its RFC 6381 string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    /// AAC, with the MPEG-4 audio object type for its `mp4a.40.OO` string.
    Aac {
        /// MPEG-4 Audio Object Type code (e.g. `2` = AAC-LC, `5` = HE-AAC), used in the `mp4a.40.OO` string.
        audio_object_type: u8,
    },
    /// Dolby Digital (AC-3).
    Ac3,
    /// Dolby Digital Plus (E-AC-3).
    Ec3,
}

impl VideoCodec {
    /// The sample-entry fourcc (e.g. `"avc1"`, `"av01"`).
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

    /// Project the first supported video sample entry into `(VideoCodec, &Visual)`.
    pub(crate) fn from_codecs(codecs: &[Codec]) -> Result<(VideoCodec, &Visual), CoreError> {
        codecs
            .iter()
            .find_map(|c| match c {
                Codec::Avc1(a) => Some((
                    VideoCodec::Avc {
                        profile: a.avcc.avc_profile_indication,
                        constraints: a.avcc.profile_compatibility,
                        level: a.avcc.avc_level_indication,
                    },
                    &a.visual,
                )),
                Codec::Av01(a) => Some((
                    VideoCodec::Av1 {
                        seq_profile: a.av1c.seq_profile,
                        seq_level_idx: a.av1c.seq_level_idx_0,
                        tier: a.av1c.seq_tier_0,
                        high_bitdepth: a.av1c.high_bitdepth,
                        twelve_bit: a.av1c.twelve_bit,
                    },
                    &a.visual,
                )),
                _ => None,
            })
            .ok_or(CoreError::UnsupportedCodec("video"))
    }
}

impl AudioCodec {
    /// The sample-entry fourcc (e.g. `"mp4a"`, `"ac-3"`, `"ec-3"`).
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

    /// Project the first supported audio sample entry into `(AudioCodec, &Audio)`.
    pub(crate) fn from_codecs(codecs: &[Codec]) -> Result<(AudioCodec, &Audio), CoreError> {
        codecs
            .iter()
            .find_map(|c| match c {
                Codec::Mp4a(a) => Some((
                    AudioCodec::Aac {
                        audio_object_type: a.esds.es_desc.dec_config.dec_specific.profile,
                    },
                    &a.audio,
                )),
                Codec::Ac3(a) => Some((AudioCodec::Ac3, &a.audio)),
                Codec::Eac3(a) => Some((AudioCodec::Ec3, &a.audio)),
                _ => None,
            })
            .ok_or(CoreError::UnsupportedCodec("audio"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avc_rfc6381_formats_profile_constraints_level_as_hex() {
        let c = VideoCodec::Avc {
            profile: 100,
            constraints: 0,
            level: 40,
        };
        assert_eq!(c.rfc6381(), "avc1.640028");
    }

    #[test]
    fn av1_rfc6381_main_tier_eight_bit() {
        let c = VideoCodec::Av1 {
            seq_profile: 0,
            seq_level_idx: 1,
            tier: false,
            high_bitdepth: false,
            twelve_bit: false,
        };
        assert_eq!(c.rfc6381(), "av01.0.01M.08");
    }

    #[test]
    fn av1_rfc6381_high_tier_ten_bit() {
        let c = VideoCodec::Av1 {
            seq_profile: 0,
            seq_level_idx: 8,
            tier: true,
            high_bitdepth: true,
            twelve_bit: false,
        };
        assert_eq!(c.rfc6381(), "av01.0.08H.10");
    }

    #[test]
    fn av1_rfc6381_twelve_bit_takes_precedence_over_high_bitdepth() {
        let c = VideoCodec::Av1 {
            seq_profile: 1,
            seq_level_idx: 0,
            tier: false,
            high_bitdepth: true,
            twelve_bit: true,
        };
        assert_eq!(c.rfc6381(), "av01.1.00M.12");
    }

    #[test]
    fn aac_rfc6381_includes_object_type() {
        assert_eq!(
            AudioCodec::Aac {
                audio_object_type: 2
            }
            .rfc6381(),
            "mp4a.40.2"
        );
    }

    #[test]
    fn ac3_rfc6381_is_the_fourcc() {
        assert_eq!(AudioCodec::Ac3.rfc6381(), "ac-3");
    }

    #[test]
    fn ec3_rfc6381_is_the_fourcc() {
        assert_eq!(AudioCodec::Ec3.rfc6381(), "ec-3");
    }

    #[test]
    fn video_from_codecs_on_empty_slice_is_unsupported() {
        let err = VideoCodec::from_codecs(&[]).unwrap_err();
        assert!(
            matches!(err, CoreError::UnsupportedCodec("video")),
            "got {err:?}"
        );
    }

    #[test]
    fn audio_from_codecs_on_empty_slice_is_unsupported() {
        let err = AudioCodec::from_codecs(&[]).unwrap_err();
        assert!(
            matches!(err, CoreError::UnsupportedCodec("audio")),
            "got {err:?}"
        );
    }
}
