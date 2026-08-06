//! What makes a run of fragments a track: the `ftyp` and `moov` at its head, and the
//! `sidx` reference that indexes each fragment.
//!
//! These have no counterpart on the way back in. A served segment is fragments and
//! nothing else, so [`unpack`](super::unpack) never reads them — a reader gets the
//! track's timescale from the probe instead.

use mp4_atom::{
    Codec, Dinf, Dref, FourCC, Ftyp, Hdlr, Mdhd, Mdia, Minf, Moov, Mvex, Mvhd, Nmhd, PlainText,
    SegmentReference, Stbl, Stco, Stsd, Tkhd, Trak, Trex, Url, VttC, Wvtt,
};

use super::{TIMESCALE, TRACK_ID};
use crate::fragmenter::Fragment;

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

/// The `sidx` reference for one fragment: what a reader indexes the track by.
pub(super) fn reference(size: usize, fragment: &Fragment) -> SegmentReference {
    SegmentReference {
        reference_type: false,
        reference_size: u32::try_from(size).expect("a fragment fits in u32 bytes"),
        subsegment_duration: fragment.duration(),
        // Every text sample can be decoded on its own.
        starts_with_sap: true,
        sap_type: 1,
        sap_delta_time: 0,
    }
}

/// The track header: one text track carrying WebVTT, declaring no language, since
/// that belongs to the transport.
pub(super) fn moov(duration: u64) -> Moov {
    Moov {
        mvhd: Mvhd {
            creation_time: 0,
            modification_time: 0,
            timescale: TIMESCALE,
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
                    timescale: TIMESCALE,
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
                            // An empty location marks the media as self-contained.
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
