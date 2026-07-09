//! Codec identity: the parsed codec-param enums, their fourcc, and RFC 6381
//! codec-string assembly. mp4-atom sample entries are projected into these enums
//! by `video_codec`/`audio_codec`; nothing here escapes cmaf.

use mp4_atom::{Audio, Codec, Visual};

use super::header::malformed;
use crate::error::Result;

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

/// Project the first supported video sample entry into `(VideoCodec, &Visual)`.
pub(crate) fn video_codec<'a>(codecs: &'a [Codec], path: &str) -> Result<(VideoCodec, &'a Visual)> {
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
        .ok_or_else(|| malformed(path, "stsd", "no supported video sample entry"))
}

/// Project the first supported audio sample entry into `(AudioCodec, &Audio)`.
pub(crate) fn audio_codec<'a>(codecs: &'a [Codec], path: &str) -> Result<(AudioCodec, &'a Audio)> {
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
        .ok_or_else(|| malformed(path, "stsd", "no supported audio sample entry"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp4_atom::{Av01, Av1c, Codec, Visual};

    #[test]
    fn extracts_avc_params_and_visual() {
        let avc1 = mp4_atom::Avc1 {
            visual: Visual {
                width: 1920,
                height: 1080,
                ..Default::default()
            },
            avcc: mp4_atom::Avcc {
                avc_profile_indication: 0x64,
                profile_compatibility: 0x00,
                avc_level_indication: 0x28,
                ..Default::default()
            },
            ..Default::default()
        };
        let codecs = [Codec::Avc1(avc1)];
        let (codec, visual) = video_codec(&codecs, "t.mp4").unwrap();
        assert_eq!(
            codec,
            VideoCodec::Avc {
                profile: 0x64,
                constraints: 0,
                level: 0x28
            }
        );
        assert_eq!((visual.width, visual.height), (1920, 1080));
    }

    #[test]
    fn extracts_av1_params() {
        let av01 = Av01 {
            visual: Visual {
                width: 320,
                height: 240,
                ..Default::default()
            },
            av1c: Av1c {
                seq_profile: 0,
                seq_level_idx_0: 5,
                seq_tier_0: false,
                high_bitdepth: false,
                twelve_bit: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let (codec, _visual) = video_codec(&[Codec::Av01(av01)], "t.mp4").unwrap();
        assert_eq!(
            codec,
            VideoCodec::Av1 {
                seq_profile: 0,
                seq_level_idx: 5,
                tier: false,
                high_bitdepth: false,
                twelve_bit: false
            }
        );
    }

    #[test]
    fn video_extraction_errors_when_no_supported_entry() {
        assert!(video_codec(&[], "t.mp4").is_err());
    }

    #[test]
    fn audio_extraction_errors_when_no_supported_entry() {
        assert!(audio_codec(&[], "t.mp4").is_err());
    }

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
