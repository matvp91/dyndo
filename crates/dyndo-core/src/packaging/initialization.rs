use mp4_atom::{
    Dinf, Dref, Hdlr, Mdhd, Mdia, Minf, Moov, Mvex, Mvhd, Nmhd, SegmentReference, Stbl, Stco, Stsd,
    Tkhd, Trak, Trex, Url,
};

use super::format::Format;
use super::{MediaSegment, PackageError};

pub(super) fn reference<P>(
    size: u32,
    segment: &MediaSegment<P>,
) -> Result<SegmentReference, PackageError> {
    Ok(SegmentReference {
        reference_type: false,
        reference_size: size,
        subsegment_duration: u32::try_from(segment.duration())
            .map_err(|_| PackageError::DurationOverflow)?,
        starts_with_sap: true,
        sap_type: 1,
        sap_delta_time: 0,
    })
}

pub(super) fn movie<F: Format>(format: &F, track_id: u32, timescale: u32, duration: u64) -> Moov {
    Moov {
        mvhd: Mvhd {
            creation_time: 0,
            modification_time: 0,
            timescale,
            duration,
            rate: 1.into(),
            volume: 1.into(),
            matrix: Default::default(),
            next_track_id: track_id.saturating_add(1),
        },
        mvex: Some(Mvex {
            mehd: None,
            trex: vec![Trex {
                track_id,
                default_sample_description_index: 1,
                ..Trex::default()
            }],
        }),
        trak: vec![Trak {
            tkhd: Tkhd {
                track_id,
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
                    handler: format.handler(),
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
                            codecs: vec![format.sample_entry()],
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
