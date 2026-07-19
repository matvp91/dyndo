//! RFC 6381 codec strings: [`rfc6381`] renders the `codecs` parameter a
//! sample entry declares (e.g. `"avc1.640028"`), [`rfc6381_sample_entry`]
//! extracts the sample-entry codingname (e.g. `"avc1"`) that DASH
//! adaptation sets and HLS rendition groups key on.

use std::fmt::Write;

use mp4_atom::{Codec, Hvcc};

use crate::error::CoreError;

/// The RFC 6381 `codecs` parameter for the sample entry `codec`
/// (e.g. `"avc1.640028"`, `"mp4a.40.2"`).
///
/// # Errors
/// [`CoreError::UnsupportedCodec`] on a sample entry dyndo does not
/// support, naming the entry.
pub fn rfc6381(codec: &Codec) -> Result<String, CoreError> {
    Ok(match codec {
        Codec::Avc1(a) => format!(
            "avc1.{:02x}{:02x}{:02x}",
            a.avcc.avc_profile_indication,
            a.avcc.profile_compatibility,
            a.avcc.avc_level_indication
        ),
        Codec::Av01(a) => {
            let t = if a.av1c.seq_tier_0 { 'H' } else { 'M' };
            let bit_depth = if a.av1c.twelve_bit {
                12
            } else if a.av1c.high_bitdepth {
                10
            } else {
                8
            };
            format!(
                "av01.{}.{:02}{t}.{bit_depth:02}",
                a.av1c.seq_profile, a.av1c.seq_level_idx_0
            )
        }
        Codec::Hvc1(a) => hevc_rfc6381("hvc1", &a.hvcc),
        Codec::Hev1(a) => hevc_rfc6381("hev1", &a.hvcc),
        Codec::Mp4a(a) => {
            // The object-type-indication is always 0x40 (MPEG-4 Audio).
            format!("mp4a.40.{}", a.esds.es_desc.dec_config.dec_specific.profile)
        }
        Codec::Ac3(_) => "ac-3".to_string(),
        Codec::Eac3(_) => "ec-3".to_string(),
        Codec::Wvtt(_) => "wvtt".to_string(),
        c => return Err(CoreError::UnsupportedCodec(codec_name(c))),
    })
}

/// The sample entry's name for error messages (e.g. `"Vp09"`): the variant
/// name off the `Debug` output — [`Codec`] offers no fourcc accessor, and
/// the full `Debug` payload is pages of decoder configuration.
fn codec_name(codec: &Codec) -> String {
    let debug = format!("{codec:?}");
    debug
        .split(['(', ' ', '{'])
        .next()
        .expect("split yields at least one item")
        .to_string()
}

/// The sample-entry codingname of an RFC 6381 `codecs` string: everything
/// before the first `.` (e.g. `"avc1"`), or the whole string for dotless
/// codecs (e.g. `"ec-3"`). Representations sharing a decoder — and thus a
/// DASH `AdaptationSet` or HLS rendition group — share this name.
pub fn rfc6381_sample_entry(rfc6381: &str) -> &str {
    rfc6381
        .split_once('.')
        .map_or(rfc6381, |(sample_entry, _)| sample_entry)
}

/// The HEVC `hvc1.…`/`hev1.…` codec string from an `hvcC` decoder
/// configuration (ISO/IEC 14496-15 Annex E).
fn hevc_rfc6381(prefix: &str, hvcc: &Hvcc) -> String {
    // profile_space: 0 → nothing, 1/2/3 → 'A'/'B'/'C'.
    let space = match hvcc.general_profile_space {
        0 => String::new(),
        n => ((b'A' + n - 1) as char).to_string(),
    };
    // Compatibility flags are emitted in reverse bit order, as hex with
    // leading zeroes suppressed.
    let flags = u32::from_be_bytes(hvcc.general_profile_compatibility_flags).reverse_bits();
    let tier = if hvcc.general_tier_flag { 'H' } else { 'L' };
    let mut s = format!(
        "{prefix}.{space}{}.{flags:x}.{tier}{}",
        hvcc.general_profile_idc, hvcc.general_level_idc
    );
    // Constraint bytes: hex, dot-separated, with trailing zero bytes dropped
    // (interior zero bytes are kept).
    let constraints = &hvcc.general_constraint_indicator_flags;
    if let Some(end) = constraints.iter().rposition(|&b| b != 0) {
        for b in &constraints[..=end] {
            write!(s, ".{b:02x}").expect("writing to a String is infallible");
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use mp4_atom::esds::{DecoderConfig, DecoderSpecific, EsDescriptor};
    use mp4_atom::{
        Ac3, Ac3SpecificBox, Audio, Av01, Av1c, Avc1, Avcc, Esds, FixedPoint, Hev1, Hvc1, Mp4a,
        PlainText, VttC, Wvtt,
    };

    use super::*;

    fn avc(profile: u8, constraints: u8, level: u8) -> String {
        rfc6381(&Codec::Avc1(Avc1 {
            avcc: Avcc {
                avc_profile_indication: profile,
                profile_compatibility: constraints,
                avc_level_indication: level,
                ..Default::default()
            },
            ..Default::default()
        }))
        .unwrap()
    }

    /// An `hvcC` with the MPEG reference vector's identity: Main profile,
    /// main tier, level 123, compatibility 0x60000000, no constraints.
    fn hvcc() -> Hvcc {
        Hvcc {
            general_profile_idc: 1,
            general_profile_compatibility_flags: [0x60, 0, 0, 0],
            general_level_idc: 123,
            ..Default::default()
        }
    }

    fn hvc1(hvcc: Hvcc) -> String {
        rfc6381(&Codec::Hvc1(Hvc1 {
            hvcc,
            ..Default::default()
        }))
        .unwrap()
    }

    fn av1(av1c: Av1c) -> String {
        rfc6381(&Codec::Av01(Av01 {
            av1c,
            ..Default::default()
        }))
        .unwrap()
    }

    fn aac(audio_object_type: u8) -> String {
        rfc6381(&Codec::Mp4a(Mp4a {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 2,
                sample_size: 16,
                sample_rate: FixedPoint::new(48_000, 0),
            },
            esds: Esds {
                es_desc: EsDescriptor {
                    dec_config: DecoderConfig {
                        dec_specific: DecoderSpecific {
                            profile: audio_object_type,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
            btrt: None,
            taic: None,
        }))
        .unwrap()
    }

    #[test]
    fn avc_renders_profile_constraints_level_as_hex() {
        assert_eq!(avc(100, 0, 40), "avc1.640028");
    }

    #[test]
    fn hevc_hvc1_renders_reference_vector() {
        // Compatibility 0x60000000 reverses to 0x6 → "6".
        assert_eq!(hvc1(hvcc()), "hvc1.1.6.L123");
    }

    #[test]
    fn hevc_hev1_uses_hev1_prefix() {
        // hvc1 and hev1 render the same fields under different codingnames —
        // the distinction DASH forbids mixing within one AdaptationSet.
        let s = rfc6381(&Codec::Hev1(Hev1 {
            hvcc: hvcc(),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(s, "hev1.1.6.L123");
    }

    #[test]
    fn hevc_reverses_compatibility_flags() {
        // 0x80000000: only the top bit set; reversed that is 0x1, proving the
        // flags are emitted in reverse bit order.
        let s = hvc1(Hvcc {
            general_profile_compatibility_flags: [0x80, 0, 0, 0],
            general_level_idc: 93,
            ..hvcc()
        });
        assert_eq!(s, "hvc1.1.1.L93");
    }

    #[test]
    fn hevc_high_tier_uses_h() {
        let s = hvc1(Hvcc {
            general_tier_flag: true,
            general_level_idc: 150,
            ..hvcc()
        });
        assert_eq!(s, "hvc1.1.6.H150");
    }

    #[test]
    fn hevc_profile_space_prefixes_a_letter() {
        // profile_space 1/2/3 → 'A'/'B'/'C' before the profile_idc.
        let s = hvc1(Hvcc {
            general_profile_space: 2,
            general_level_idc: 93,
            ..hvcc()
        });
        assert_eq!(s, "hvc1.B1.6.L93");
    }

    #[test]
    fn hevc_keeps_interior_zero_constraint_bytes_but_trims_trailing() {
        let s = hvc1(Hvcc {
            general_constraint_indicator_flags: [0x90, 0x00, 0x50, 0, 0, 0],
            general_level_idc: 120,
            ..hvcc()
        });
        assert_eq!(s, "hvc1.1.6.L120.90.00.50");
    }

    #[test]
    fn av1_renders_main_tier_eight_bit() {
        let s = av1(Av1c {
            seq_level_idx_0: 1,
            ..Default::default()
        });
        assert_eq!(s, "av01.0.01M.08");
    }

    #[test]
    fn av1_twelve_bit_takes_precedence_over_high_bitdepth() {
        let s = av1(Av1c {
            seq_profile: 1,
            high_bitdepth: true,
            twelve_bit: true,
            ..Default::default()
        });
        assert_eq!(s, "av01.1.00M.12");
    }

    #[test]
    fn aac_renders_object_type() {
        assert_eq!(aac(2), "mp4a.40.2");
    }

    #[test]
    fn ac3_renders_its_fourcc() {
        let s = rfc6381(&Codec::Ac3(Ac3 {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 6,
                sample_size: 16,
                sample_rate: FixedPoint::new(48_000, 0),
            },
            dac3: Ac3SpecificBox {
                fscod: 0,
                bsid: 8,
                bsmod: 0,
                acmod: 7,
                lfeon: true,
                bit_rate_code: 8,
            },
        }))
        .unwrap();
        assert_eq!(s, "ac-3");
    }

    #[test]
    fn wvtt_renders_its_fourcc() {
        let s = rfc6381(&Codec::Wvtt(Wvtt {
            plaintext: PlainText {
                data_reference_index: 1,
            },
            config: VttC {
                config: String::new(),
            },
            label: None,
            btrt: None,
        }))
        .unwrap();
        assert_eq!(s, "wvtt");
    }

    #[test]
    fn an_unsupported_sample_entry_errors_with_its_name() {
        let err = rfc6381(&Codec::Vp09(Default::default())).unwrap_err();
        assert_eq!(err.to_string(), "unsupported codec: Vp09");
    }

    #[test]
    fn sample_entry_is_the_prefix_before_the_first_dot() {
        assert_eq!(rfc6381_sample_entry("avc1.640028"), "avc1");
        assert_eq!(rfc6381_sample_entry("mp4a.40.2"), "mp4a");
        assert_eq!(rfc6381_sample_entry("hev1.1.6.L123"), "hev1");
    }

    #[test]
    fn sample_entry_of_a_dotless_codec_is_the_whole_string() {
        assert_eq!(rfc6381_sample_entry("ec-3"), "ec-3");
        assert_eq!(rfc6381_sample_entry("wvtt"), "wvtt");
    }
}
