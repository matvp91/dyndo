//! RFC 6381 codec identifiers. [`Codec`] pairs the sample-entry codingname
//! (`id`, e.g. `"avc1"`) with the profile/level parameters that follow it
//! (e.g. `"640028"`). It renders to and parses back from the single `codecs`
//! string a sample entry declares, and serde treats it as that string.
//! [`Codec::from_moov`] builds one from a track's `moov`.

use std::fmt::{self, Write as _};

use mp4_atom::{Codec as SampleEntry, Hvcc, Moov};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// An RFC 6381 codec: a sample-entry codingname and its parameters.
///
/// The [`Display`](fmt::Display) form is the `codecs` parameter DASH and HLS
/// advertise (e.g. `"avc1.640028"`, `"mp4a.40.2"`, `"ec-3"`); the
/// [`id`](Codec::id) alone is the codingname adaptation sets and rendition
/// groups key on. serde (de)serializes it as that single string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", from = "String")]
pub struct Codec {
    /// The sample-entry codingname (e.g. `"avc1"`, `"mp4a"`, `"ec-3"`):
    /// everything before the first `.` of the RFC 6381 string.
    pub id: String,
    /// The parameters following the codingname (e.g. `"640028"`, `"40.2"`),
    /// or `None` for a codec that declares none (e.g. `"ec-3"`, `"wvtt"`).
    pub parameters: Option<String>,
}

impl Codec {
    /// The codec a track's `moov` declares, from its first sample entry.
    ///
    /// # Errors
    /// [`CoreError::UnsupportedCodec`] on a sample entry dyndo does not
    /// support, naming the entry.
    pub fn from_moov(moov: &Moov) -> Result<Codec, CoreError> {
        Codec::from_sample_entry(&moov.trak[0].mdia.minf.stbl.stsd.codecs[0])
    }

    /// The codec a sample entry declares (e.g. an `avc1` entry → the codec
    /// rendering as `"avc1.640028"`).
    ///
    /// # Errors
    /// [`CoreError::UnsupportedCodec`] on a sample entry dyndo does not
    /// support, naming the entry.
    fn from_sample_entry(entry: &SampleEntry) -> Result<Codec, CoreError> {
        Ok(match entry {
            SampleEntry::Avc1(a) => Codec {
                id: "avc1".to_string(),
                parameters: Some(format!(
                    "{:02x}{:02x}{:02x}",
                    a.avcc.avc_profile_indication,
                    a.avcc.profile_compatibility,
                    a.avcc.avc_level_indication
                )),
            },
            SampleEntry::Av01(a) => {
                let tier = if a.av1c.seq_tier_0 { 'H' } else { 'M' };
                let bit_depth = if a.av1c.twelve_bit {
                    12
                } else if a.av1c.high_bitdepth {
                    10
                } else {
                    8
                };
                Codec {
                    id: "av01".to_string(),
                    parameters: Some(format!(
                        "{}.{:02}{tier}.{bit_depth:02}",
                        a.av1c.seq_profile, a.av1c.seq_level_idx_0
                    )),
                }
            }
            SampleEntry::Hvc1(a) => hevc("hvc1", &a.hvcc),
            SampleEntry::Hev1(a) => hevc("hev1", &a.hvcc),
            SampleEntry::Mp4a(a) => Codec {
                id: "mp4a".to_string(),
                // The object-type-indication is always 0x40 (MPEG-4 Audio).
                parameters: Some(format!(
                    "40.{}",
                    a.esds.es_desc.dec_config.dec_specific.profile
                )),
            },
            SampleEntry::Ac3(_) => Codec {
                id: "ac-3".to_string(),
                parameters: None,
            },
            SampleEntry::Eac3(_) => Codec {
                id: "ec-3".to_string(),
                parameters: None,
            },
            SampleEntry::Wvtt(_) => Codec {
                id: "wvtt".to_string(),
                parameters: None,
            },
            entry => return Err(CoreError::UnsupportedCodec(codec_name(entry))),
        })
    }
}

impl fmt::Display for Codec {
    /// Rejoin the codingname and parameters with a `.` (just the codingname
    /// when there are none): the RFC 6381 `codecs` string.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id)?;
        if let Some(parameters) = &self.parameters {
            write!(f, ".{parameters}")?;
        }
        Ok(())
    }
}

impl From<Codec> for String {
    fn from(codec: Codec) -> String {
        codec.to_string()
    }
}

impl From<String> for Codec {
    /// Split an RFC 6381 string on its first `.` into the codingname `id`
    /// and the remaining `parameters` (`None` for a dotless codec).
    fn from(codec: String) -> Codec {
        match codec.split_once('.') {
            Some((id, parameters)) => Codec {
                id: id.to_string(),
                parameters: Some(parameters.to_string()),
            },
            None => Codec {
                id: codec,
                parameters: None,
            },
        }
    }
}

/// The HEVC codec (`hvc1`/`hev1`) from an `hvcC` decoder configuration
/// (ISO/IEC 14496-15 Annex E): `prefix` is the codingname, the parameters
/// carry the profile, compatibility flags, tier, level, and constraint bytes.
fn hevc(prefix: &str, hvcc: &Hvcc) -> Codec {
    // profile_space: 0 → nothing, 1/2/3 → 'A'/'B'/'C'.
    let space = match hvcc.general_profile_space {
        0 => String::new(),
        n => ((b'A' + n - 1) as char).to_string(),
    };
    // Compatibility flags are emitted in reverse bit order, as hex with
    // leading zeroes suppressed.
    let flags = u32::from_be_bytes(hvcc.general_profile_compatibility_flags).reverse_bits();
    let tier = if hvcc.general_tier_flag { 'H' } else { 'L' };
    let mut parameters = format!(
        "{space}{}.{flags:x}.{tier}{}",
        hvcc.general_profile_idc, hvcc.general_level_idc
    );
    // Constraint bytes: hex, dot-separated, with trailing zero bytes dropped
    // (interior zero bytes are kept).
    let constraints = &hvcc.general_constraint_indicator_flags;
    if let Some(end) = constraints.iter().rposition(|&b| b != 0) {
        for b in &constraints[..=end] {
            write!(parameters, ".{b:02x}").expect("writing to a String is infallible");
        }
    }
    Codec {
        id: prefix.to_string(),
        parameters: Some(parameters),
    }
}

/// The sample entry's variant name for error messages (e.g. `"Vp09"`): taken
/// off the `Debug` output, as the sample entry offers no codingname accessor
/// and its full `Debug` payload is pages of decoder configuration.
fn codec_name(entry: &SampleEntry) -> String {
    let debug = format!("{entry:?}");
    debug
        .split(['(', ' ', '{'])
        .next()
        .expect("split yields at least one item")
        .to_string()
}

#[cfg(test)]
mod tests {
    use mp4_atom::esds::{DecoderConfig, DecoderSpecific, EsDescriptor};
    use mp4_atom::{
        Ac3, Ac3SpecificBox, Audio, Av01, Av1c, Avc1, Avcc, Codec as SampleEntry, Esds, FixedPoint,
        Hev1, Hvc1, Mp4a, PlainText, VttC, Wvtt,
    };

    use super::*;

    fn avc(profile: u8, constraints: u8, level: u8) -> String {
        Codec::from_sample_entry(&SampleEntry::Avc1(Avc1 {
            avcc: Avcc {
                avc_profile_indication: profile,
                profile_compatibility: constraints,
                avc_level_indication: level,
                ..Default::default()
            },
            ..Default::default()
        }))
        .unwrap()
        .to_string()
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
        Codec::from_sample_entry(&SampleEntry::Hvc1(Hvc1 {
            hvcc,
            ..Default::default()
        }))
        .unwrap()
        .to_string()
    }

    fn av1(av1c: Av1c) -> String {
        Codec::from_sample_entry(&SampleEntry::Av01(Av01 {
            av1c,
            ..Default::default()
        }))
        .unwrap()
        .to_string()
    }

    fn aac(audio_object_type: u8) -> String {
        Codec::from_sample_entry(&SampleEntry::Mp4a(Mp4a {
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
        .to_string()
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
        let s = Codec::from_sample_entry(&SampleEntry::Hev1(Hev1 {
            hvcc: hvcc(),
            ..Default::default()
        }))
        .unwrap()
        .to_string();
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
    fn ac3_renders_its_codingname() {
        let s = Codec::from_sample_entry(&SampleEntry::Ac3(Ac3 {
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
        .unwrap()
        .to_string();
        assert_eq!(s, "ac-3");
    }

    #[test]
    fn wvtt_renders_its_codingname() {
        let s = Codec::from_sample_entry(&SampleEntry::Wvtt(Wvtt {
            plaintext: PlainText {
                data_reference_index: 1,
            },
            config: VttC {
                config: String::new(),
            },
            label: None,
            btrt: None,
        }))
        .unwrap()
        .to_string();
        assert_eq!(s, "wvtt");
    }

    #[test]
    fn an_unsupported_sample_entry_errors_with_its_name() {
        let err = Codec::from_sample_entry(&SampleEntry::Vp09(Default::default())).unwrap_err();
        assert_eq!(err.to_string(), "unsupported codec: Vp09");
    }

    #[test]
    fn id_is_the_codingname_before_the_first_dot() {
        assert_eq!(Codec::from("avc1.640028".to_string()).id, "avc1");
        assert_eq!(Codec::from("mp4a.40.2".to_string()).id, "mp4a");
        assert_eq!(Codec::from("hev1.1.6.L123".to_string()).id, "hev1");
    }

    #[test]
    fn a_dotless_codec_parses_to_a_bare_codingname() {
        for codingname in ["ec-3", "wvtt"] {
            let codec = Codec::from(codingname.to_string());
            assert_eq!(codec.id, codingname);
            assert_eq!(codec.parameters, None);
        }
    }

    #[test]
    fn display_rejoins_id_and_parameters() {
        assert_eq!(Codec::from("mp4a.40.2".to_string()).to_string(), "mp4a.40.2");
        assert_eq!(Codec::from("wvtt".to_string()).to_string(), "wvtt");
    }

    #[test]
    fn serde_round_trips_through_the_codec_string() {
        let codec = Codec {
            id: "avc1".to_string(),
            parameters: Some("640028".to_string()),
        };
        let json = serde_json::to_string(&codec).unwrap();
        assert_eq!(json, r#""avc1.640028""#);
        assert_eq!(serde_json::from_str::<Codec>(&json).unwrap(), codec);
    }
}
