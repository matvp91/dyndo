//! Pack per-segment cue windows (see
//! [`Subtitle::expand`](super::subtitle::Subtitle::expand)) into a fragmented
//! CMAF `wvtt` track (ISO/IEC 14496-30).

use mp4_atom::{
    Codec, Dinf, Dref, Encode, Ftyp, Hdlr, Mdat, Mdhd, Mdia, Mfhd, Minf, Moof, Moov, Mvex, Mvhd,
    Nmhd, PlainText, SegmentReference, Sidx, Stbl, Stco, Stsd, Styp, Tfdt, Tfhd, Tkhd, Traf, Trak,
    Trex, Trun, TrunEntry, Url, VttC, Wvtt,
};

use super::error::CoreTextError;
use super::subtitle::Cue;
use crate::asset::Segment;

/// Media timescale for packed tracks (ms map 1:1 to media units).
const TIMESCALE: u32 = 1000;

/// Append a box header `[u32 size][fourcc]` for a body of `body_len` bytes
/// (size includes the 8-byte header); the caller writes the body itself.
fn push_header(out: &mut Vec<u8>, fourcc: [u8; 4], body_len: usize) {
    let size = (8 + body_len) as u32;
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(&fourcc);
}

/// Group `cues` into samples (consecutive cues sharing `[start, end]`) and
/// return the `trun` entries plus the concatenated `mdat` bytes. A lone
/// empty-`text` cue → one `vtte`; otherwise one `vttc{payl}` per cue. Box
/// sizes are known from the text lengths, so samples are written straight
/// into `mdat` (entry sizes are length deltas) with no per-sample scratch.
fn encode_samples(cues: &[Cue]) -> (Vec<TrunEntry>, Vec<u8>) {
    let mut entries = Vec::new();
    let mut mdat = Vec::new();

    let mut i = 0;
    while i < cues.len() {
        let (start, end) = (cues[i].start_ms, cues[i].end_ms);
        let mut j = i;
        while j < cues.len() && cues[j].start_ms == start && cues[j].end_ms == end {
            j += 1;
        }
        let group = &cues[i..j];

        let sample_start = mdat.len();
        if group.len() == 1 && group[0].text.is_empty() {
            push_header(&mut mdat, *b"vtte", 0);
        } else {
            for c in group {
                // A vttc wrapping a single payl box that holds the cue text.
                push_header(&mut mdat, *b"vttc", 8 + c.text.len());
                push_header(&mut mdat, *b"payl", c.text.len());
                mdat.extend_from_slice(c.text.as_bytes());
            }
        }

        entries.push(TrunEntry {
            duration: Some((end - start) as u32),
            size: Some((mdat.len() - sample_start) as u32),
            flags: None,
            cts: None,
        });
        i = j;
    }
    (entries, mdat)
}

/// Pack per-segment cue `windows` into a fragmented CMAF `wvtt` track — one
/// CMAF segment per `(window, segment)` pair (equal length by construction;
/// see [`Subtitle::expand`](super::subtitle::Subtitle::expand)). Per-sample
/// durations come from cue extents; per-segment decode time and `sidx`
/// duration come from `segment.duration_ms`. `language` is written into the
/// track's `mdhd`.
///
/// Emits `ftyp` · `moov` · `sidx` · then per segment `styp` · `moof` · `mdat`.
///
/// # Errors
/// [`CoreTextError::Wvtt`] if any box fails to encode.
pub fn pack(
    language: &str,
    windows: &[Vec<Cue>],
    segments: &[Segment],
) -> Result<Vec<u8>, CoreTextError> {
    let track_duration: u64 = segments.iter().map(|s| s.duration_ms).sum();

    // The styp is identical for every segment: encode it once and splice the
    // bytes in per segment.
    let mut styp_bytes = Vec::new();
    Styp {
        major_brand: b"msdh".into(),
        minor_version: 0,
        compatible_brands: vec![b"msdh".into(), b"msix".into(), b"cmfs".into()],
    }
    .encode(&mut styp_bytes)
    .map_err(|e| CoreTextError::Wvtt(e.to_string()))?;

    let mut media = Vec::new();
    let mut seg_refs: Vec<SegmentReference> = Vec::new();
    let mut decode_time = 0u64;
    // Scratch for the moof pre-encode that sizes `data_offset`; reused across
    // segments so only the first iteration allocates.
    let mut scratch = Vec::new();
    for (i, (cues, seg)) in windows.iter().zip(segments).enumerate() {
        let (entries, mdat_data) = encode_samples(cues);

        let mut moof = Moof {
            mfhd: Mfhd {
                sequence_number: (i + 1) as u32,
            },
            traf: vec![Traf {
                tfhd: Tfhd {
                    track_id: 1,
                    default_base_is_moof: true,
                    ..Default::default()
                },
                // decode_time equals the first cue's start by construction, but
                // is independent of whether the window has any cue.
                tfdt: Some(Tfdt {
                    base_media_decode_time: decode_time,
                }),
                trun: vec![Trun {
                    data_offset: Some(0),
                    entries,
                }],
                ..Default::default()
            }],
        };
        scratch.clear();
        moof.encode(&mut scratch)
            .map_err(|e| CoreTextError::Wvtt(e.to_string()))?;
        moof.traf[0].trun[0].data_offset = Some((scratch.len() + 8) as i32);

        // Encode the segment straight into `media`; its sidx reference size is
        // the length delta, so no per-segment buffer is assembled and copied.
        let seg_start = media.len();
        media.extend_from_slice(&styp_bytes);
        moof.encode(&mut media)
            .map_err(|e| CoreTextError::Wvtt(e.to_string()))?;
        Mdat { data: mdat_data }
            .encode(&mut media)
            .map_err(|e| CoreTextError::Wvtt(e.to_string()))?;

        seg_refs.push(SegmentReference {
            reference_type: false,
            reference_size: (media.len() - seg_start) as u32,
            subsegment_duration: seg.duration_ms as u32,
            starts_with_sap: true,
            sap_type: 1,
            sap_delta_time: 0,
        });
        decode_time += seg.duration_ms;
    }

    let sample_entry = Wvtt {
        plaintext: PlainText {
            data_reference_index: 1,
        },
        config: VttC {
            config: "WEBVTT\n".to_string(),
        },
        label: None,
        btrt: None,
    };

    let moov = Moov {
        mvhd: Mvhd {
            timescale: TIMESCALE,
            duration: track_duration,
            next_track_id: 2,
            ..Default::default()
        },
        mvex: Some(Mvex {
            mehd: None,
            trex: vec![Trex {
                track_id: 1,
                default_sample_description_index: 1,
                ..Default::default()
            }],
        }),
        trak: vec![Trak {
            tkhd: Tkhd {
                track_id: 1,
                duration: track_duration,
                enabled: true,
                ..Default::default()
            },
            mdia: Mdia {
                mdhd: Mdhd {
                    timescale: TIMESCALE,
                    duration: track_duration,
                    language: language.to_string(),
                    ..Default::default()
                },
                hdlr: Hdlr {
                    handler: b"text".into(),
                    name: "dyndo WebVTT".into(),
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
                            codecs: vec![Codec::Wvtt(sample_entry)],
                        },
                        stco: Some(Stco::default()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
            ..Default::default()
        }],
        ..Default::default()
    };

    let sidx = Sidx {
        reference_id: 1,
        timescale: TIMESCALE,
        earliest_presentation_time: 0,
        first_offset: 0,
        references: seg_refs,
    };

    let mut out = Vec::new();
    Ftyp {
        major_brand: b"iso6".into(),
        minor_version: 0,
        compatible_brands: vec![b"iso6".into(), b"cmfc".into(), b"cmft".into()],
    }
    .encode(&mut out)
    .map_err(|e| CoreTextError::Wvtt(e.to_string()))?;
    moov.encode(&mut out)
        .map_err(|e| CoreTextError::Wvtt(e.to_string()))?;
    sidx.encode(&mut out)
        .map_err(|e| CoreTextError::Wvtt(e.to_string()))?;
    out.extend_from_slice(&media);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::subtitle::Subtitle;

    fn seg_ms(duration_ms: u64) -> Segment {
        Segment {
            offset: 0,
            size: 0,
            duration: 0,
            duration_ms,
        }
    }

    #[test]
    fn pack_round_trips_through_mp4_atom() {
        use mp4_atom::{Any, Codec, DecodeMaybe, FourCC};

        let subtitle = Subtitle {
            language: "eng".to_string(),
            cues: vec![
                Cue {
                    start_ms: 0,
                    end_ms: 2000,
                    text: "Hello".into(),
                },
                Cue {
                    start_ms: 5000,
                    end_ms: 7000,
                    text: "World".into(),
                },
            ],
        };
        // Two 4000ms segments (a video timeline): windows [0,4000) and [4000,8000).
        let segments = vec![seg_ms(4000), seg_ms(4000)];
        let windows = subtitle.expand(&segments);
        let bytes = pack(&subtitle.language, &windows, &segments).unwrap();

        let mut buf = bytes.as_slice();
        let mut kinds = Vec::new();
        let mut moov = None;
        let mut sidx = None;
        while let Some(any) = Any::decode_maybe(&mut buf).unwrap() {
            kinds.push(any.kind());
            match any {
                Any::Moov(m) => moov = Some(m),
                Any::Sidx(s) => sidx = Some(s),
                _ => {}
            }
        }

        assert_eq!(kinds[0], FourCC::new(b"ftyp"));
        assert_eq!(kinds[1], FourCC::new(b"moov"));
        assert_eq!(kinds[2], FourCC::new(b"sidx"));
        assert_eq!(kinds[3], FourCC::new(b"styp"));

        let moov = moov.unwrap();
        assert_eq!(moov.trak[0].mdia.mdhd.language, "eng");
        assert!(matches!(
            moov.trak[0].mdia.minf.stbl.stsd.codecs[0],
            Codec::Wvtt(_)
        ));

        // Two segments in, two out.
        let seg_count = kinds.iter().filter(|k| **k == FourCC::new(b"styp")).count();
        assert_eq!(seg_count, 2);
        assert_eq!(sidx.unwrap().references.len(), seg_count);
    }

    #[test]
    fn cue_sample_is_vttc_payl_and_gap_is_vtte() {
        let subtitle = Subtitle {
            language: "und".to_string(),
            cues: vec![Cue {
                start_ms: 1000,
                end_ms: 2000,
                text: "Hi".into(),
            }],
        };
        // One 2000ms segment: [0,1000) gap, [1000,2000) cue.
        let segments = vec![seg_ms(2000)];
        let windows = subtitle.expand(&segments);
        let bytes = pack(&subtitle.language, &windows, &segments).unwrap();
        assert!(bytes.windows(4).any(|w| w == b"vtte"));
        assert!(bytes.windows(4).any(|w| w == b"vttc"));
        assert!(bytes.windows(4).any(|w| w == b"payl"));
        assert!(!bytes.windows(4).any(|w| w == b"vsid"));
        assert!(!bytes.windows(4).any(|w| w == b"iden"));
        assert!(!bytes.windows(4).any(|w| w == b"sttg"));
    }

    #[test]
    fn sample_durations_in_a_window_sum_to_its_span() {
        // A fully-tiled 2000ms window: gap [0,1000) + cue [1000,2000).
        let cues = vec![
            Cue {
                start_ms: 0,
                end_ms: 1000,
                text: String::new(),
            },
            Cue {
                start_ms: 1000,
                end_ms: 2000,
                text: "Hi".into(),
            },
        ];
        let (entries, _mdat) = encode_samples(&cues);
        let total: u64 = entries.iter().map(|e| e.duration.unwrap() as u64).sum();
        assert_eq!(total, 2000);
    }
}
