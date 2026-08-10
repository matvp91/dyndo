use mp4_atom::{
    Codec, Dinf, Dref, FourCC, Ftyp, Hdlr, Mdhd, Mdia, Minf, Moov, Mvex, Mvhd, Nmhd, PlainText,
    SegmentReference, Stbl, Stco, Stsd, Tkhd, Trak, Trex, Url, VttC, Wvtt,
};

use super::super::TimedFragment;
use super::{TRACK_ID, WvttSample};

pub(super) fn ftyp() -> Ftyp {
    Ftyp {
        major_brand: FourCC::new(b"iso6"),
        minor_version: 0,
        compatible_brands: vec![
            FourCC::new(b"iso6"),
            FourCC::new(b"cmfc"),
            FourCC::new(b"cmft"),
        ],
    }
}

pub(super) fn reference(
    size: u32,
    fragment: &TimedFragment<WvttSample>,
) -> Result<SegmentReference, super::PackageError> {
    Ok(SegmentReference {
        reference_type: false,
        reference_size: size,
        subsegment_duration: u32::try_from(fragment.duration())
            .map_err(|_| super::PackageError::DurationOverflow)?,
        starts_with_sap: true,
        sap_type: 1,
        sap_delta_time: 0,
    })
}

pub(super) fn moov(timescale: u32, duration: u64) -> Moov {
    Moov {
        mvhd: Mvhd {
            creation_time: 0,
            modification_time: 0,
            timescale,
            duration,
            rate: 1.into(),
            volume: 1.into(),
            matrix: Default::default(),
            next_track_id: TRACK_ID + 1,
        },
        mvex: Some(Mvex {
            mehd: None,
            trex: vec![Trex {
                track_id: TRACK_ID,
                default_sample_description_index: 1,
                ..Trex::default()
            }],
        }),
        trak: vec![Trak {
            tkhd: Tkhd {
                track_id: TRACK_ID,
                duration,
                enabled: true,
                in_movie: true,
                ..Tkhd::default()
            },
            mdia: Mdia {
                mdhd: Mdhd {
                    timescale,
                    duration,
                    language: "und".to_string(),
                    ..Mdhd::default()
                },
                hdlr: Hdlr {
                    handler: FourCC::new(b"text"),
                    name: "dyndo".to_string(),
                },
                minf: Minf {
                    nmhd: Some(Nmhd {}),
                    dinf: Dinf {
                        dref: Dref {
                            urls: vec![Url {
                                location: String::new(),
                            }],
                        },
                    },
                    stbl: Stbl {
                        stsd: Stsd {
                            codecs: vec![Codec::Wvtt(Wvtt {
                                plaintext: PlainText {
                                    data_reference_index: 1,
                                },
                                config: VttC {
                                    config: "WEBVTT\n".to_string(),
                                },
                                label: None,
                                btrt: None,
                            })],
                        },
                        stco: Some(Stco::default()),
                        ..Stbl::default()
                    },
                    ..Minf::default()
                },
            },
            ..Trak::default()
        }],
        ..Moov::default()
    }
}
