//! Pack a [`Subtitle`] into a fragmented CMAF `wvtt` track (ISO/IEC 14496-30).

use mp4_atom::{
    Codec, Dinf, Dref, Encode, Ftyp, Hdlr, Mdat, Mdhd, Mdia, Mfhd, Minf, Moof, Moov, Mvex, Mvhd,
    Nmhd, PlainText, SegmentReference, Sidx, Stbl, Stco, Stsd, Styp, Tfdt, Tfhd, Tkhd, Traf, Trak,
    Trex, Trun, TrunEntry, Url, VttC, Wvtt,
};

use super::error::CoreTextError;
use super::subtitle::Subtitle;
use super::subtitle_chunk::{self, SubtitleChunk};

/// Media timescale for packed tracks (ms map 1:1 to media units).
const TIMESCALE: u32 = 1000;

/// Append a box `[u32 size][fourcc][body]` (size includes the 8-byte header).
fn push_box(out: &mut Vec<u8>, fourcc: &[u8; 4], body: &[u8]) {
    let size = (8 + body.len()) as u32;
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(fourcc);
    out.extend_from_slice(body);
}

fn wvtt_err(e: mp4_atom::Error) -> CoreTextError {
    CoreTextError::Wvtt(e.to_string())
}

/// The first sample's start time (= chunk start), or 0 if the chunk is empty.
fn chunk_start(chunk: &SubtitleChunk) -> u64 {
    chunk.cues.first().map(|c| c.start_ms).unwrap_or(0)
}

/// Group a chunk's cues into samples (consecutive cues sharing `[start, end]`)
/// and return the `trun` entries, the concatenated `mdat` bytes, and the chunk's
/// total duration. A lone empty-`text` cue → one `vtte`; otherwise one
/// `vttc{payl}` per cue.
fn encode_chunk(chunk: &SubtitleChunk) -> (Vec<TrunEntry>, Vec<u8>, u64) {
    let cues = &chunk.cues;
    let mut entries = Vec::new();
    let mut mdat = Vec::new();
    let mut duration = 0u64;

    let mut i = 0;
    while i < cues.len() {
        let (start, end) = (cues[i].start_ms, cues[i].end_ms);
        let mut j = i;
        while j < cues.len() && cues[j].start_ms == start && cues[j].end_ms == end {
            j += 1;
        }
        let group = &cues[i..j];

        let mut bytes = Vec::new();
        if group.len() == 1 && group[0].text.is_empty() {
            push_box(&mut bytes, b"vtte", &[]);
        } else {
            for c in group {
                let mut body = Vec::new();
                push_box(&mut body, b"payl", c.text.as_bytes());
                push_box(&mut bytes, b"vttc", &body);
            }
        }

        let sample_duration = end - start;
        entries.push(TrunEntry {
            duration: Some(sample_duration as u32),
            size: Some(bytes.len() as u32),
            flags: None,
            cts: None,
        });
        duration += sample_duration;
        mdat.extend_from_slice(&bytes);
        i = j;
    }
    (entries, mdat, duration)
}

/// Pack a `Subtitle` into a fragmented CMAF `wvtt` track.
///
/// Chunks the cues (via [`subtitle_chunk::chunk`]) into `chunk_duration_ms`
/// windows, then emits `ftyp` · `moov` · `sidx` · per-chunk `styp` · `moof` ·
/// `mdat`. The `Subtitle`'s `language` is written into the track's `mdhd`.
///
/// # Errors
/// [`CoreTextError::Wvtt`] if any box fails to encode.
pub fn pack(subtitle: &Subtitle, chunk_duration_ms: u64) -> Result<Vec<u8>, CoreTextError> {
    let chunks = subtitle_chunk::chunk(subtitle, chunk_duration_ms);
    let track_duration = subtitle.cues.iter().map(|c| c.end_ms).max().unwrap_or(0);

    let mut media = Vec::new();
    let mut seg_refs: Vec<SegmentReference> = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let (entries, mdat_data, seg_duration) = encode_chunk(chunk);

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
                    base_media_decode_time: chunk_start(chunk),
                }),
                trun: vec![Trun {
                    data_offset: Some(0),
                    entries,
                }],
                ..Default::default()
            }],
        };
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
                    language: subtitle.language.clone(),
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
    use crate::text::subtitle::Cue;

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
        let bytes = pack(&subtitle, 4000).unwrap();

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

        // 0-2s and 5-7s over 4s windows → windows [0,4) and [4,7) → 2 chunks.
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
        // One chunk; timeline [0,2000): gap [0,1000), cue [1000,2000).
        let bytes = pack(&subtitle, 100_000).unwrap();
        assert!(bytes.windows(4).any(|w| w == b"vtte"));
        assert!(bytes.windows(4).any(|w| w == b"vttc"));
        assert!(bytes.windows(4).any(|w| w == b"payl"));
        assert!(!bytes.windows(4).any(|w| w == b"vsid"));
        assert!(!bytes.windows(4).any(|w| w == b"iden"));
        assert!(!bytes.windows(4).any(|w| w == b"sttg"));
    }
}
