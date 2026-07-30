//! RFC 6381 codec identifiers.

use std::fmt::Write as _;

use mp4_atom::{Codec, Hvcc, Moov};

use relative_path::RelativePath;

use super::error::{Error, InvalidTrack};

/// Return the RFC 6381 codec string declared by the first sample entry in
/// `moov`.
///
/// # Errors
/// Returns an error when the sample entry is not supported.
pub fn codec_from_moov(moov: &Moov, path: &RelativePath) -> Result<String, Error> {
    let entry = &moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    Ok(match entry {
        Codec::Avc1(entry) => format!(
            "avc1.{:02x}{:02x}{:02x}",
            entry.avcc.avc_profile_indication,
            entry.avcc.profile_compatibility,
            entry.avcc.avc_level_indication
        ),
        Codec::Av01(entry) => {
            let tier = if entry.av1c.seq_tier_0 { 'H' } else { 'M' };
            let bit_depth = if entry.av1c.twelve_bit {
                12
            } else if entry.av1c.high_bitdepth {
                10
            } else {
                8
            };
            format!(
                "av01.{}.{:02}{tier}.{bit_depth:02}",
                entry.av1c.seq_profile, entry.av1c.seq_level_idx_0
            )
        }
        Codec::Hvc1(entry) => hevc("hvc1", &entry.hvcc),
        Codec::Hev1(entry) => hevc("hev1", &entry.hvcc),
        Codec::Mp4a(entry) => format!(
            "mp4a.40.{}",
            entry.esds.es_desc.dec_config.dec_specific.profile
        ),
        Codec::Ac3(_) => "ac-3".to_string(),
        Codec::Eac3(_) => "ec-3".to_string(),
        Codec::Wvtt(_) => "wvtt".to_string(),
        entry => {
            return Err(Error::InvalidTrack {
                path: path.to_owned(),
                reason: InvalidTrack::UnsupportedCodec {
                    codec: codec_name(entry),
                },
            });
        }
    })
}

fn hevc(coding_name: &str, hvcc: &Hvcc) -> String {
    let profile_space = match hvcc.general_profile_space {
        0 => String::new(),
        value => ((b'A' + value - 1) as char).to_string(),
    };
    let compatibility = u32::from_be_bytes(hvcc.general_profile_compatibility_flags).reverse_bits();
    let tier = if hvcc.general_tier_flag { 'H' } else { 'L' };
    let mut codec = format!(
        "{coding_name}.{profile_space}{}.{compatibility:x}.{tier}{}",
        hvcc.general_profile_idc, hvcc.general_level_idc
    );

    if let Some(end) = hvcc
        .general_constraint_indicator_flags
        .iter()
        .rposition(|&byte| byte != 0)
    {
        for byte in &hvcc.general_constraint_indicator_flags[..=end] {
            write!(codec, ".{byte:02x}").expect("writing to a String is infallible");
        }
    }

    codec
}

fn codec_name(entry: &Codec) -> String {
    let debug = format!("{entry:?}");
    debug
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or(debug.as_str())
        .to_string()
}
