//! Supported codecs and their RFC 6381 strings. Temporarily copied from
//! `crate::cmaf::codec`; to be unified later.

use mp4_atom::{Audio, Codec, Visual};

/// A supported video codec with the parameters needed for its RFC 6381 string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// A supported audio codec with the parameters needed for its RFC 6381 string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Aac { audio_object_type: u8 },
    Ac3,
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
    pub(crate) fn from_codecs(codecs: &[Codec]) -> (VideoCodec, &Visual) {
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
            .unwrap()
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
    pub(crate) fn from_codecs(codecs: &[Codec]) -> (AudioCodec, &Audio) {
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
            .unwrap()
    }
}
