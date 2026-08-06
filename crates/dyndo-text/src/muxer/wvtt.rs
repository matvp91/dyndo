//! Packing a fragmented subtitle into a CMAF `wvtt` track (ISO/IEC 14496-30).

use mp4_atom::{
    BufMut, Codec, Dinf, Dref, Encode, FourCC, Ftyp, Hdlr, Mdat, Mdhd, Mdia, Mfhd, Minf, Moof,
    Moov, Mvex, Mvhd, Nmhd, PlainText, SegmentReference, Sidx, Stbl, Stco, Stsd, Styp, Tfdt, Tfhd,
    Tkhd, Traf, Trak, Trex, Trun, TrunEntry, Url, VttC, Wvtt,
};

use super::atoms::{Payl, Vttc, Vtte};
use crate::fragmenter::{Fragment, Sample};

/// Milliseconds map 1:1 onto media time.
const TIMESCALE: u32 = 1_000;

const TRACK_ID: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum WvttError {
    #[error("subtitle covers no time")]
    Empty,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}

/// Pack fragments of a subtitle into a single fragmented `wvtt` track: `ftyp` ·
/// `moov` · `sidx` · one `styp` · `moof` · `mdat` per fragment. The `sidx` sits
/// ahead of the fragments and gives each one's size and duration, so a reader can
/// index the whole track from the head of the file.
///
/// Divide a subtitle with [`fragment`](crate::fragmenter::fragment) to get
/// fragments ending on the asset's clock, so every text track of an asset carries
/// the same fragment timeline and stays segment-aligned with its siblings. The
/// track runs to the end of the last fragment.
///
/// Each sample is written as one `vttc` per cue on screen over it, or as a single
/// `vtte` where no cue is — the empty box the format spends on an interval
/// showing nothing. That the samples tile their fragment without holes is what
/// makes the track legal.
///
/// The track declares no language; that belongs to the transport.
///
/// # Errors
///
/// [`WvttError::Empty`] if the fragments run to time 0, since the result would be
/// a track with nothing to index, and [`WvttError::Atom`] if a box fails to
/// encode.
pub fn pack(fragments: &[Fragment]) -> Result<Vec<u8>, WvttError> {
    let track_end = fragments.last().map_or(0, |fragment| fragment.end);
    if track_end == 0 {
        return Err(WvttError::Empty);
    }

    let mut encoded = Vec::with_capacity(fragments.len());
    let mut references = Vec::with_capacity(fragments.len());
    for (index, fragment) in fragments.iter().enumerate() {
        let bytes = encode(index, fragment)?;
        references.push(reference(bytes.len(), fragment));
        encoded.push(bytes);
    }

    let mut track = Vec::new();
    ftyp().encode(&mut track)?;
    moov(track_end).encode(&mut track)?;
    Sidx {
        reference_id: TRACK_ID,
        timescale: TIMESCALE,
        earliest_presentation_time: 0,
        first_offset: 0,
        references,
    }
    .encode(&mut track)?;
    for fragment in encoded {
        track.extend_from_slice(&fragment);
    }

    Ok(track)
}

/// The cues on screen over a sample, each a `vttc` carrying its text. An
/// interval showing nothing is a lone `vtte`, which the format still spends a
/// sample on.
fn write_sample<B: BufMut>(sample: &Sample, buf: &mut B) -> mp4_atom::Result<()> {
    if sample.cues.is_empty() {
        return Vtte.encode(buf);
    }

    for cue in &sample.cues {
        Vttc {
            payl: Payl {
                text: cue.text.clone(),
            },
        }
        .encode(buf)?;
    }

    Ok(())
}

fn encode(index: usize, fragment: &Fragment) -> Result<Vec<u8>, WvttError> {
    let mut data = Vec::new();
    let mut entries = Vec::with_capacity(fragment.samples.len());
    for sample in &fragment.samples {
        let offset = data.len();
        write_sample(sample, &mut data)?;
        entries.push(TrunEntry {
            duration: Some(milliseconds(sample.duration())),
            size: Some(u32::try_from(data.len() - offset).expect("a sample fits in u32 bytes")),
            ..TrunEntry::default()
        });
    }

    let mut moof = Moof {
        mfhd: Mfhd {
            sequence_number: u32::try_from(index + 1)
                .expect("a track has fewer than u32 fragments"),
        },
        traf: vec![Traf {
            tfhd: Tfhd {
                track_id: TRACK_ID,
                default_base_is_moof: true,
                ..Tfhd::default()
            },
            tfdt: Some(Tfdt {
                base_media_decode_time: fragment.start,
            }),
            trun: vec![Trun {
                data_offset: Some(0),
                entries,
            }],
            ..Traf::default()
        }],
    };

    let mut bytes = Vec::new();
    styp().encode(&mut bytes)?;

    // `default_base_is_moof` anchors the offset at the moof, and the samples
    // start just past the mdat header. Encoding it twice measures the moof
    // without changing its length: the offset field is present either way.
    let moof_start = bytes.len();
    moof.encode(&mut bytes)?;
    let data_offset = bytes.len() - moof_start + 8;
    moof.traf[0].trun[0].data_offset =
        Some(i32::try_from(data_offset).expect("a fragment fits in i32 bytes"));
    bytes.truncate(moof_start);
    moof.encode(&mut bytes)?;

    Mdat { data }.encode(&mut bytes)?;

    Ok(bytes)
}

fn reference(size: usize, fragment: &Fragment) -> SegmentReference {
    SegmentReference {
        reference_type: false,
        reference_size: u32::try_from(size).expect("a fragment fits in u32 bytes"),
        subsegment_duration: milliseconds(fragment.duration()),
        // Every text sample can be decoded on its own.
        starts_with_sap: true,
        sap_type: 1,
        sap_delta_time: 0,
    }
}

fn milliseconds(duration: u64) -> u32 {
    u32::try_from(duration).expect("a track is shorter than u32 milliseconds")
}

fn ftyp() -> Ftyp {
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

fn styp() -> Styp {
    Styp {
        major_brand: FourCC::new(b"msdh"),
        minor_version: 0,
        compatible_brands: vec![
            FourCC::new(b"msdh"),
            FourCC::new(b"msix"),
            FourCC::new(b"cmfs"),
        ],
    }
}

fn moov(duration: u64) -> Moov {
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

#[cfg(test)]
mod tests {
    use mp4_atom::{Buf, Decode};

    use super::*;
    use crate::fragmenter::fragment;
    use crate::subtitle::{Cue, Subtitle};

    #[test]
    fn packs_an_indexable_track() {
        let subtitle = subtitle(&[(0, 1_000, "A")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();

        let mut buf = track.as_slice();
        let ftyp = Ftyp::decode(&mut buf).unwrap();
        let moov = Moov::decode(&mut buf).unwrap();
        let sidx = Sidx::decode(&mut buf).unwrap();

        assert_eq!(
            (
                ftyp.major_brand,
                moov.trak.len(),
                sidx.timescale,
                sidx.earliest_presentation_time,
                sidx.references.len()
            ),
            (FourCC::new(b"iso6"), 1, TIMESCALE, 0, 1)
        );
    }

    #[test]
    fn describes_a_text_track_carrying_webvtt() {
        let subtitle = subtitle(&[(0, 1_000, "A")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();
        let moov = moov_of(&track);
        let mdia = &moov.trak[0].mdia;

        let Codec::Wvtt(wvtt) = &mdia.minf.stbl.stsd.codecs[0] else {
            panic!("expected a wvtt sample entry");
        };
        assert_eq!(
            (
                mdia.hdlr.handler,
                mdia.mdhd.timescale,
                mdia.mdhd.language.as_str(),
                wvtt.config.config.as_str()
            ),
            (FourCC::new(b"text"), TIMESCALE, "und", "WEBVTT\n")
        );
    }

    #[test]
    fn declares_the_track_duration_from_the_last_cue() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (4_000, 6_500, "B")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();
        let moov = moov_of(&track);

        assert_eq!(
            (moov.mvhd.duration, moov.trak[0].mdia.mdhd.duration),
            (6_500, 6_500)
        );
    }

    #[test]
    fn every_reference_matches_its_fragment() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]);
        let track = pack(&fragment(&subtitle, &[], 1_000)).unwrap();
        let (sidx, fragments) = fragments_of(&track);

        assert_eq!(
            sidx.references
                .iter()
                .map(|reference| (reference.reference_size, reference.subsegment_duration))
                .collect::<Vec<_>>(),
            fragments
                .iter()
                .map(|fragment| (fragment.size, fragment.duration))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn references_are_random_access_media() {
        let subtitle = subtitle(&[(0, 1_000, "A")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();
        let (sidx, _) = fragments_of(&track);

        let reference = &sidx.references[0];
        assert_eq!(
            (
                reference.reference_type,
                reference.starts_with_sap,
                reference.sap_type
            ),
            (false, true, 1)
        );
    }

    #[test]
    fn a_gap_becomes_an_empty_cue_sample() {
        let subtitle = subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();
        let (_, fragments) = fragments_of(&track);

        let gap = &fragments[0].samples[1];
        assert_eq!(Vtte::decode(&mut gap.bytes.as_slice()).unwrap(), Vtte);
    }

    #[test]
    fn a_cue_sample_carries_its_text() {
        let subtitle = subtitle(&[(0, 1_000, "Hello")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();
        let (_, fragments) = fragments_of(&track);

        let sample = &fragments[0].samples[0];
        assert_eq!(
            Vttc::decode(&mut sample.bytes.as_slice()).unwrap(),
            Vttc {
                payl: Payl {
                    text: "Hello".to_string()
                }
            }
        );
    }

    #[test]
    fn simultaneous_cues_share_one_sample() {
        let subtitle = subtitle(&[(0, 2_000, "A"), (1_000, 3_000, "B")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();
        let (_, fragments) = fragments_of(&track);

        // [0,1000)=A, [1000,2000)=A+B, [2000,3000)=B
        let overlap = &mut fragments[0].samples[1].bytes.as_slice();
        assert_eq!(
            [
                Vttc::decode(overlap).unwrap().payl.text,
                Vttc::decode(overlap).unwrap().payl.text
            ],
            ["A".to_string(), "B".to_string()]
        );
    }

    #[test]
    fn each_fragment_declares_its_own_start_and_duration() {
        let subtitle = subtitle(&[(0, 10_000, "A")]);
        let track = pack(&fragment(&subtitle, &[7_400], 4_000)).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert_eq!(
            fragments
                .iter()
                .map(|fragment| (fragment.start, fragment.duration))
                .collect::<Vec<_>>(),
            [(0, 4_000), (4_000, 3_400), (7_400, 600), (8_000, 2_000)]
        );
    }

    #[test]
    fn samples_follow_the_mdat_header() {
        let subtitle = subtitle(&[(0, 1_000, "A")]);
        let track = pack(&fragment(&subtitle, &[], 0)).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert_eq!(
            fragments[0].data_offset,
            i32::try_from(fragments[0].moof_size + 8).unwrap()
        );
    }

    #[test]
    fn a_subtitle_without_cues_is_rejected() {
        let subtitle = subtitle(&[]);
        let error = pack(&fragment(&subtitle, &[], 0)).unwrap_err();

        assert!(matches!(error, WvttError::Empty));
    }

    #[test]
    fn a_subtitle_ending_at_time_zero_is_rejected() {
        let subtitle = subtitle(&[(0, 0, "A")]);
        let error = pack(&fragment(&subtitle, &[], 0)).unwrap_err();

        assert!(matches!(error, WvttError::Empty));
    }

    fn subtitle(cues: &[(u64, u64, &str)]) -> Subtitle {
        Subtitle {
            cues: cues
                .iter()
                .map(|&(start, end, text)| Cue {
                    start,
                    end,
                    text: text.to_string(),
                })
                .collect(),
        }
    }

    struct Fragment {
        start: u64,
        duration: u32,
        size: u32,
        moof_size: usize,
        data_offset: i32,
        samples: Vec<FragmentSample>,
    }

    struct FragmentSample {
        bytes: Vec<u8>,
    }

    fn moov_of(track: &[u8]) -> Moov {
        let mut buf = track;
        Ftyp::decode(&mut buf).unwrap();
        Moov::decode(&mut buf).unwrap()
    }

    fn fragments_of(track: &[u8]) -> (Sidx, Vec<Fragment>) {
        let mut buf = track;
        Ftyp::decode(&mut buf).unwrap();
        Moov::decode(&mut buf).unwrap();
        let sidx = Sidx::decode(&mut buf).unwrap();

        let mut fragments = Vec::new();
        while buf.remaining() > 0 {
            let size = buf.remaining();
            Styp::decode(&mut buf).unwrap();
            let before_moof = buf.remaining();
            let moof = Moof::decode(&mut buf).unwrap();
            let moof_size = before_moof - buf.remaining();
            let mdat = Mdat::decode(&mut buf).unwrap();
            let traf = &moof.traf[0];
            let trun = &traf.trun[0];

            let mut offset = 0;
            let samples = trun
                .entries
                .iter()
                .map(|entry| {
                    let size = entry.size.unwrap() as usize;
                    let sample = FragmentSample {
                        bytes: mdat.data[offset..offset + size].to_vec(),
                    };
                    offset += size;
                    sample
                })
                .collect();

            fragments.push(Fragment {
                start: traf.tfdt.as_ref().unwrap().base_media_decode_time,
                duration: trun
                    .entries
                    .iter()
                    .map(|entry| entry.duration.unwrap())
                    .sum(),
                size: u32::try_from(size - buf.remaining()).unwrap(),
                moof_size,
                data_offset: trun.data_offset.unwrap(),
                samples,
            });
        }

        (sidx, fragments)
    }
}
