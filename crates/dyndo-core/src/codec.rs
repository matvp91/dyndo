//! Supported codecs and the parameters needed for their RFC 6381 strings.

use mp4_atom::{Audio, Codec, Hvcc, Visual};

use crate::CoreError;

/// Which kind of media track a codec belongs to; the discriminant carried by
/// [`CoreError::UnsupportedCodec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    /// A video track.
    Video,
    /// An audio track.
    Audio,
    /// A text (subtitle/caption) track.
    Text,
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MediaType::Video => "video",
            MediaType::Audio => "audio",
            MediaType::Text => "text",
        })
    }
}

/// Which HEVC sample entry a track uses. The two differ only in where the
/// parameter sets live (in the sample entry for `hvc1`, in-band for `hev1`);
/// this selects the fourcc and the RFC 6381 codec-string prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HevcEntry {
    /// `hvc1`: parameter sets carried in the sample entry.
    Hvc1,
    /// `hev1`: parameter sets carried in-band.
    Hev1,
}

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
    /// H.265/HEVC, with the fields from its `hvcC` needed for the
    /// `hvc1.…`/`hev1.…` string (ISO/IEC 14496-15 Annex E).
    Hevc {
        /// Which sample entry (`hvc1` vs `hev1`) the track uses.
        entry: HevcEntry,
        /// `general_profile_space` (0 = no prefix, 1–3 = `A`/`B`/`C`).
        profile_space: u8,
        /// `general_tier_flag`: `false` = main (`L`), `true` = high (`H`).
        tier: bool,
        /// `general_profile_idc`.
        profile_idc: u8,
        /// `general_profile_compatibility_flags` (32 bits, big-endian bytes).
        compatibility_flags: [u8; 4],
        /// `general_constraint_indicator_flags` (48 bits, big-endian bytes).
        constraint_flags: [u8; 6],
        /// `general_level_idc`.
        level_idc: u8,
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

/// Project an `hvcC` decoder-configuration record into a [`VideoCodec::Hevc`],
/// tagged with the sample entry (`hvc1`/`hev1`) it came from.
fn hevc_codec(entry: HevcEntry, hvcc: &Hvcc) -> VideoCodec {
    VideoCodec::Hevc {
        entry,
        profile_space: hvcc.general_profile_space,
        tier: hvcc.general_tier_flag,
        profile_idc: hvcc.general_profile_idc,
        compatibility_flags: hvcc.general_profile_compatibility_flags,
        constraint_flags: hvcc.general_constraint_indicator_flags,
        level_idc: hvcc.general_level_idc,
    }
}

impl VideoCodec {
    /// The sample-entry fourcc (e.g. `"avc1"`, `"av01"`).
    pub fn fourcc(&self) -> &'static str {
        match self {
            VideoCodec::Avc { .. } => "avc1",
            VideoCodec::Av1 { .. } => "av01",
            VideoCodec::Hevc {
                entry: HevcEntry::Hvc1,
                ..
            } => "hvc1",
            VideoCodec::Hevc {
                entry: HevcEntry::Hev1,
                ..
            } => "hev1",
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
            VideoCodec::Hevc {
                entry,
                profile_space,
                tier,
                profile_idc,
                compatibility_flags,
                constraint_flags,
                level_idc,
            } => {
                let prefix = match entry {
                    HevcEntry::Hvc1 => "hvc1",
                    HevcEntry::Hev1 => "hev1",
                };
                // profile_space: 0 → nothing, 1/2/3 → 'A'/'B'/'C'.
                let space = match profile_space {
                    0 => String::new(),
                    n => ((b'A' + n - 1) as char).to_string(),
                };
                // Compatibility flags are emitted in reverse bit order, as hex
                // with leading zeroes suppressed.
                let flags = u32::from_be_bytes(compatibility_flags).reverse_bits();
                let tier = if tier { 'H' } else { 'L' };
                let mut s = format!("{prefix}.{space}{profile_idc}.{flags:x}.{tier}{level_idc}");
                // Constraint bytes: hex, dot-separated, with trailing zero bytes
                // dropped (interior zero bytes are kept).
                if let Some(end) = constraint_flags.iter().rposition(|&b| b != 0) {
                    for b in &constraint_flags[..=end] {
                        s.push_str(&format!(".{b:02x}"));
                    }
                }
                s
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
                Codec::Hvc1(a) => Some((hevc_codec(HevcEntry::Hvc1, &a.hvcc), &a.visual)),
                Codec::Hev1(a) => Some((hevc_codec(HevcEntry::Hev1, &a.hvcc), &a.visual)),
                _ => None,
            })
            .ok_or(CoreError::UnsupportedCodec(MediaType::Video))
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
            .ok_or(CoreError::UnsupportedCodec(MediaType::Audio))
    }
}

/// A supported timed-text codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCodec {
    /// WebVTT in ISO-BMFF (`wvtt`), per ISO/IEC 14496-30.
    Wvtt,
}

impl TextCodec {
    /// The sample-entry fourcc (`"wvtt"`).
    pub fn fourcc(&self) -> &'static str {
        match self {
            TextCodec::Wvtt => "wvtt",
        }
    }

    /// RFC 6381 codecs parameter (`"wvtt"`).
    pub fn rfc6381(&self) -> String {
        match self {
            TextCodec::Wvtt => "wvtt".to_string(),
        }
    }

    /// Project the first supported timed-text sample entry into a [`TextCodec`].
    pub(crate) fn from_codecs(codecs: &[Codec]) -> Result<TextCodec, CoreError> {
        codecs
            .iter()
            .find_map(|c| match c {
                Codec::Wvtt(_) => Some(TextCodec::Wvtt),
                _ => None,
            })
            .ok_or(CoreError::UnsupportedCodec(MediaType::Text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wvtt_fourcc_and_rfc6381() {
        assert_eq!(TextCodec::Wvtt.fourcc(), "wvtt");
        assert_eq!(TextCodec::Wvtt.rfc6381(), "wvtt");
    }

    #[test]
    fn text_from_codecs_on_empty_slice_is_unsupported() {
        let err = TextCodec::from_codecs(&[]).unwrap_err();
        assert!(
            matches!(err, CoreError::UnsupportedCodec(MediaType::Text)),
            "got {err:?}"
        );
    }

    #[test]
    fn hevc_hvc1_fourcc_is_hvc1() {
        let c = VideoCodec::Hevc {
            entry: HevcEntry::Hvc1,
            profile_space: 0,
            tier: false,
            profile_idc: 1,
            compatibility_flags: [0x60, 0, 0, 0],
            constraint_flags: [0; 6],
            level_idc: 123,
        };
        assert_eq!(c.fourcc(), "hvc1");
    }

    #[test]
    fn hevc_hev1_fourcc_is_hev1() {
        let c = VideoCodec::Hevc {
            entry: HevcEntry::Hev1,
            profile_space: 0,
            tier: false,
            profile_idc: 1,
            compatibility_flags: [0x60, 0, 0, 0],
            constraint_flags: [0; 6],
            level_idc: 123,
        };
        assert_eq!(c.fourcc(), "hev1");
    }

    /// Build an HEVC codec with the given overrides, defaulting the rest to the
    /// MPEG reference vector (Main profile, tier main, level 123, no constraints).
    fn hevc(entry: HevcEntry) -> VideoCodec {
        VideoCodec::Hevc {
            entry,
            profile_space: 0,
            tier: false,
            profile_idc: 1,
            compatibility_flags: [0x60, 0, 0, 0],
            constraint_flags: [0; 6],
            level_idc: 123,
        }
    }

    #[test]
    fn hevc_rfc6381_hvc1_main_tier() {
        // mp4-atom's MPEG vector: compat 0x60000000 reverses to 0x6 → "6".
        assert_eq!(hevc(HevcEntry::Hvc1).rfc6381(), "hvc1.1.6.L123");
    }

    #[test]
    fn hevc_rfc6381_hev1_uses_hev1_prefix() {
        assert_eq!(hevc(HevcEntry::Hev1).rfc6381(), "hev1.1.6.L123");
    }

    #[test]
    fn hevc_rfc6381_reverses_compatibility_flags() {
        // 0x80000000: only the top bit set; reversed that is 0x1 → "1", proving
        // the flags are emitted in reverse bit order.
        let c = VideoCodec::Hevc {
            entry: HevcEntry::Hvc1,
            profile_space: 0,
            tier: false,
            profile_idc: 1,
            compatibility_flags: [0x80, 0, 0, 0],
            constraint_flags: [0; 6],
            level_idc: 93,
        };
        assert_eq!(c.rfc6381(), "hvc1.1.1.L93");
    }

    #[test]
    fn hevc_rfc6381_high_tier_uses_h() {
        let c = VideoCodec::Hevc {
            entry: HevcEntry::Hvc1,
            profile_space: 0,
            tier: true,
            profile_idc: 1,
            compatibility_flags: [0x60, 0, 0, 0],
            constraint_flags: [0; 6],
            level_idc: 150,
        };
        assert_eq!(c.rfc6381(), "hvc1.1.6.H150");
    }

    #[test]
    fn hevc_rfc6381_profile_space_prefixes_a_letter() {
        // profile_space 1/2/3 → 'A'/'B'/'C' before the profile_idc.
        let c = VideoCodec::Hevc {
            entry: HevcEntry::Hvc1,
            profile_space: 2,
            tier: false,
            profile_idc: 1,
            compatibility_flags: [0x60, 0, 0, 0],
            constraint_flags: [0; 6],
            level_idc: 93,
        };
        assert_eq!(c.rfc6381(), "hvc1.B1.6.L93");
    }

    #[test]
    fn hevc_rfc6381_appends_nonzero_constraint_byte() {
        // mp4-atom's libheif vector: constraint flags 0x90… → trailing ".90".
        let c = VideoCodec::Hevc {
            entry: HevcEntry::Hvc1,
            profile_space: 0,
            tier: false,
            profile_idc: 1,
            compatibility_flags: [0x60, 0, 0, 0],
            constraint_flags: [0x90, 0, 0, 0, 0, 0],
            level_idc: 120,
        };
        assert_eq!(c.rfc6381(), "hvc1.1.6.L120.90");
    }

    #[test]
    fn hevc_rfc6381_keeps_interior_zero_constraint_bytes_but_trims_trailing() {
        // Only trailing zero bytes are dropped; the interior 0x00 stays.
        let c = VideoCodec::Hevc {
            entry: HevcEntry::Hvc1,
            profile_space: 0,
            tier: false,
            profile_idc: 1,
            compatibility_flags: [0x60, 0, 0, 0],
            constraint_flags: [0x90, 0x00, 0x50, 0, 0, 0],
            level_idc: 120,
        };
        assert_eq!(c.rfc6381(), "hvc1.1.6.L120.90.00.50");
    }

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
            matches!(err, CoreError::UnsupportedCodec(MediaType::Video)),
            "got {err:?}"
        );
    }

    #[test]
    fn audio_from_codecs_on_empty_slice_is_unsupported() {
        let err = AudioCodec::from_codecs(&[]).unwrap_err();
        assert!(
            matches!(err, CoreError::UnsupportedCodec(MediaType::Audio)),
            "got {err:?}"
        );
    }
}
