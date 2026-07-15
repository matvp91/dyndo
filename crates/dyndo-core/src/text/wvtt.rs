//! Pack a parsed WebVTT document into a fragmented CMAF `wvtt` track
//! (ISO/IEC 14496-30).

use std::collections::BTreeSet;

use mp4_atom::{
    Codec, Dinf, Dref, Encode, Ftyp, Hdlr, Mdat, Mdhd, Mdia, Mfhd, Minf, Moof, Moov, Mvex, Mvhd,
    Nmhd, PlainText, SegmentReference, Sidx, Stbl, Stco, Stsd, Styp, Tfdt, Tfhd, Tkhd, Traf, Trak,
    Trex, Trun, TrunEntry, Url, VttC, Wvtt,
};

use super::error::CoreTextError;
use super::vtt::WebVtt;
use super::vtt_cue::VttCue;

/// Media timescale for packed tracks. WebVTT is millisecond-precise, so ms map
/// 1:1 to media units.
const TIMESCALE: u32 = 1000;

/// One presentation interval of the tiled timeline: a contiguous span during
/// which a fixed set of cues (possibly none) is active.
struct Sample {
    start_ms: u64,
    duration_ms: u64,
    cues: Vec<SampleCue>,
}

/// One cue as it appears in a single [`Sample`].
struct SampleCue {
    /// `Some` when the cue is split across more than one sample; fragments of
    /// the same original cue share this id (emitted as a `vsid` box).
    source_id: Option<i32>,
    id: Option<String>,
    settings: Option<String>,
    payload: String,
}

/// Tile `cues` into a gapless, non-overlapping, presentation-ordered sample
/// stream. `segment_duration_ms` seeds the boundary set with the segment grid so
/// no sample ever straddles a segment boundary.
fn build_samples(cues: &[VttCue], segment_duration_ms: u64) -> Vec<Sample> {
    if cues.is_empty() {
        return Vec::new();
    }
    let track_end = cues.iter().map(|c| c.end_ms).max().unwrap_or(0);

    let mut bounds: BTreeSet<u64> = BTreeSet::new();
    bounds.insert(0);
    bounds.insert(track_end);
    for c in cues {
        bounds.insert(c.start_ms);
        bounds.insert(c.end_ms);
    }
    if segment_duration_ms > 0 {
        let mut t = segment_duration_ms;
        while t < track_end {
            bounds.insert(t);
            t += segment_duration_ms;
        }
    }
    let bounds: Vec<u64> = bounds.into_iter().collect();

    // Fragment count per cue → assign a source id only to cues that span >1 sample.
    let mut frag_count = vec![0usize; cues.len()];
    for w in bounds.windows(2) {
        for (i, c) in cues.iter().enumerate() {
            if c.start_ms <= w[0] && c.end_ms >= w[1] {
                frag_count[i] += 1;
            }
        }
    }
    let mut source_ids: Vec<Option<i32>> = vec![None; cues.len()];
    let mut next_id: i32 = 1;
    for (i, count) in frag_count.iter().enumerate() {
        if *count > 1 {
            source_ids[i] = Some(next_id);
            next_id += 1;
        }
    }

    let mut samples = Vec::with_capacity(bounds.len().saturating_sub(1));
    for w in bounds.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        let mut sample_cues = Vec::new();
        for (i, c) in cues.iter().enumerate() {
            if c.start_ms <= t0 && c.end_ms >= t1 {
                sample_cues.push(SampleCue {
                    source_id: source_ids[i],
                    id: c.id.clone(),
                    settings: c.settings.clone(),
                    payload: c.payload.clone(),
                });
            }
        }
        samples.push(Sample {
            start_ms: t0,
            duration_ms: t1 - t0,
            cues: sample_cues,
        });
    }
    samples
}

/// Append a box `[u32 size][fourcc][body]` (size includes the 8-byte header).
fn push_box(out: &mut Vec<u8>, fourcc: &[u8; 4], body: &[u8]) {
    let size = (8 + body.len()) as u32;
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(fourcc);
    out.extend_from_slice(body);
}

/// Encode a sample's media-data bytes: a `vtte` for an empty sample, otherwise
/// one `vttc` per cue (`vsid` → `iden` → `sttg` → `payl`).
fn encode_sample(sample: &Sample) -> Vec<u8> {
    let mut out = Vec::new();
    if sample.cues.is_empty() {
        push_box(&mut out, b"vtte", &[]);
        return out;
    }
    for cue in &sample.cues {
        let mut body = Vec::new();
        if let Some(sid) = cue.source_id {
            push_box(&mut body, b"vsid", &sid.to_be_bytes());
        }
        if let Some(id) = &cue.id {
            push_box(&mut body, b"iden", id.as_bytes());
        }
        if let Some(settings) = &cue.settings {
            push_box(&mut body, b"sttg", settings.as_bytes());
        }
        push_box(&mut body, b"payl", cue.payload.as_bytes());
        push_box(&mut out, b"vttc", &body);
    }
    out
}

fn wvtt_err(e: mp4_atom::Error) -> CoreTextError {
    CoreTextError::Wvtt(e.to_string())
}

/// Pack a parsed WebVTT document into a fragmented CMAF `wvtt` track.
///
/// Produces `ftyp` · `moov` · `sidx` · then, per `segment_duration_ms` segment,
/// `styp` · `moof` · `mdat`, implementing the ISO 14496-30 sample model.
/// `language` is an ISO-639-2 code stored in `mdhd`.
///
/// # Errors
/// Returns [`CoreTextError::Wvtt`] if any box fails to encode.
pub fn pack(
    vtt: &WebVtt,
    segment_duration_ms: u64,
    language: &str,
) -> Result<Vec<u8>, CoreTextError> {
    let samples = build_samples(&vtt.cues, segment_duration_ms);
    let track_duration: u64 = samples.iter().map(|s| s.duration_ms).sum();

    // Group consecutive samples by segment index. No sample straddles a
    // boundary, so a change in `start_ms / seg` starts a new segment.
    let seg = segment_duration_ms.max(1);
    let mut segments: Vec<Vec<&Sample>> = Vec::new();
    let mut cur_idx: Option<u64> = None;
    for s in &samples {
        let idx = s.start_ms / seg;
        if Some(idx) != cur_idx {
            segments.push(Vec::new());
            cur_idx = Some(idx);
        }
        segments.last_mut().expect("just pushed").push(s);
    }

    // Build each segment's styp+moof+mdat bytes; record size + duration for sidx.
    let mut media = Vec::new();
    let mut seg_refs: Vec<SegmentReference> = Vec::new();
    for (i, group) in segments.iter().enumerate() {
        let seg_start = group[0].start_ms;
        let seg_duration: u64 = group.iter().map(|s| s.duration_ms).sum();

        let mut mdat_data = Vec::new();
        let mut entries = Vec::with_capacity(group.len());
        for s in group {
            let bytes = encode_sample(s);
            entries.push(TrunEntry {
                duration: Some(s.duration_ms as u32),
                size: Some(bytes.len() as u32),
                flags: None,
                cts: None,
            });
            mdat_data.extend_from_slice(&bytes);
        }

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
                tfdt: Some(Tfdt {
                    base_media_decode_time: seg_start,
                }),
                trun: vec![Trun {
                    data_offset: Some(0),
                    entries,
                }],
                ..Default::default()
            }],
        };
        // data_offset points from the moof start to the mdat payload.
        let mut scratch = Vec::new();
        moof.encode(&mut scratch).map_err(wvtt_err)?;
        moof.traf[0].trun[0].data_offset = Some((scratch.len() + 8) as i32);

        let mut seg_bytes = Vec::new();
        Styp {
            major_brand: b"msdh".into(),
            minor_version: 0,
            compatible_brands: vec![b"msdh".into(), b"msix".into(), b"cmfs".into()],
        }
        .encode(&mut seg_bytes)
        .map_err(wvtt_err)?;
        moof.encode(&mut seg_bytes).map_err(wvtt_err)?;
        Mdat { data: mdat_data }
            .encode(&mut seg_bytes)
            .map_err(wvtt_err)?;

        seg_refs.push(SegmentReference {
            reference_type: false,
            reference_size: seg_bytes.len() as u32,
            subsegment_duration: seg_duration as u32,
            starts_with_sap: true,
            sap_type: 1,
            sap_delta_time: 0,
        });
        media.extend_from_slice(&seg_bytes);
    }

    let sample_entry = Wvtt {
        plaintext: PlainText {
            data_reference_index: 1,
        },
        config: VttC {
            config: vtt.config.clone(),
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
    .map_err(wvtt_err)?;
    moov.encode(&mut out).map_err(wvtt_err)?;
    sidx.encode(&mut out).map_err(wvtt_err)?;
    out.extend_from_slice(&media);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start_ms: u64, end_ms: u64, payload: &str) -> VttCue {
        VttCue {
            id: None,
            start_ms,
            end_ms,
            settings: None,
            payload: payload.into(),
        }
    }

    fn sample(cues: Vec<SampleCue>) -> Sample {
        Sample {
            start_ms: 0,
            duration_ms: 100,
            cues,
        }
    }

    #[test]
    fn empty_document_yields_no_samples() {
        assert!(build_samples(&[], 4000).is_empty());
    }

    #[test]
    fn sequential_cues_get_gap_fill_and_no_source_id() {
        let cues = vec![cue(0, 1000, "A"), cue(2000, 3000, "B")];
        let s = build_samples(&cues, 100_000);
        // Intervals: [0,1000)=A, [1000,2000)=gap, [2000,3000)=B.
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].cues.len(), 1);
        assert_eq!(s[0].cues[0].payload, "A");
        assert_eq!(s[0].cues[0].source_id, None);
        assert!(s[1].cues.is_empty());
        assert_eq!(s[1].duration_ms, 1000);
        assert_eq!(s[2].cues[0].payload, "B");
    }

    #[test]
    fn overlapping_cues_split_with_shared_source_id() {
        let cues = vec![cue(0, 5000, "A"), cue(3000, 8000, "B")];
        let s = build_samples(&cues, 100_000);
        // Intervals: [0,3000)=A, [3000,5000)=A+B, [5000,8000)=B.
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].cues.len(), 1);
        assert_eq!(s[1].cues.len(), 2);
        assert_eq!(s[2].cues.len(), 1);
        let a0 = s[0].cues[0].source_id.unwrap();
        let a1 = s[1]
            .cues
            .iter()
            .find(|c| c.payload == "A")
            .unwrap()
            .source_id
            .unwrap();
        assert_eq!(a0, a1);
        let b1 = s[1]
            .cues
            .iter()
            .find(|c| c.payload == "B")
            .unwrap()
            .source_id
            .unwrap();
        let b2 = s[2].cues[0].source_id.unwrap();
        assert_eq!(b1, b2);
        assert_ne!(a0, b1);
    }

    #[test]
    fn cue_crossing_segment_boundary_splits_with_shared_source_id() {
        let cues = vec![cue(2000, 6000, "A")];
        let s = build_samples(&cues, 4000);
        // Intervals: [0,2000)=gap, [2000,4000)=A, [4000,6000)=A (split at seg 4000).
        assert_eq!(s.len(), 3);
        assert!(s[0].cues.is_empty());
        assert_eq!(s[1].cues[0].payload, "A");
        assert_eq!(s[2].cues[0].payload, "A");
        assert_eq!(
            s[1].cues[0].source_id.unwrap(),
            s[2].cues[0].source_id.unwrap()
        );
    }

    #[test]
    fn empty_sample_encodes_as_vtte() {
        let bytes = encode_sample(&sample(vec![]));
        assert_eq!(bytes, vec![0, 0, 0, 8, b'v', b't', b't', b'e']);
    }

    #[test]
    fn cue_sample_encodes_vttc_with_payl() {
        let bytes = encode_sample(&sample(vec![SampleCue {
            source_id: None,
            id: None,
            settings: None,
            payload: "Hi".into(),
        }]));
        let mut expected = Vec::new();
        expected.extend_from_slice(&18u32.to_be_bytes()); // vttc: 8 + payl(10)
        expected.extend_from_slice(b"vttc");
        expected.extend_from_slice(&10u32.to_be_bytes()); // payl: 8 + 2
        expected.extend_from_slice(b"payl");
        expected.extend_from_slice(b"Hi");
        assert_eq!(bytes, expected);
    }

    #[test]
    fn cue_sample_orders_boxes_and_embeds_source_id() {
        let bytes = encode_sample(&sample(vec![SampleCue {
            source_id: Some(7),
            id: Some("c1".into()),
            settings: Some("align:start".into()),
            payload: "Hi".into(),
        }]));
        let pos = |fourcc: &[u8; 4]| bytes.windows(4).position(|w| w == fourcc).unwrap();
        assert!(pos(b"vsid") < pos(b"iden"));
        assert!(pos(b"iden") < pos(b"sttg"));
        assert!(pos(b"sttg") < pos(b"payl"));
        let vsid = pos(b"vsid");
        assert_eq!(&bytes[vsid + 4..vsid + 8], &7i32.to_be_bytes());
    }

    #[test]
    fn pack_round_trips_through_mp4_atom() {
        use mp4_atom::{Any, Codec, DecodeMaybe, FourCC};

        let doc = "WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n\n00:05.000 --> 00:07.000\nWorld";
        let vtt = crate::text::parse(doc).unwrap();
        let bytes = pack(&vtt, 4000, "und").unwrap();

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
        assert!(matches!(
            moov.trak[0].mdia.minf.stbl.stsd.codecs[0],
            Codec::Wvtt(_)
        ));

        // Cues 0-2s and 5-7s with 4s segments span 0-7s → segments [0,4) and [4,7).
        let seg_count = kinds.iter().filter(|k| **k == FourCC::new(b"styp")).count();
        assert_eq!(seg_count, 2);
        assert_eq!(sidx.unwrap().references.len(), seg_count);
    }
}
