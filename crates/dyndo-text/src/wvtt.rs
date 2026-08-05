//! Packing a [`Subtitle`] into a CMAF `wvtt` track (ISO/IEC 14496-30).

use std::ops::Range;

use mp4_atom::{
    Atom, Buf, BufMut, Codec, Decode, Dinf, Dref, Encode, FourCC, Ftyp, Hdlr, Mdat, Mdhd, Mdia,
    Mfhd, Minf, Moof, Moov, Mvex, Mvhd, Nmhd, PlainText, SegmentReference, Sidx, Stbl, Stco, Stsd,
    Styp, Tfdt, Tfhd, Tkhd, Traf, Trak, Trex, Trun, TrunEntry, Url, VttC, Wvtt,
};

use crate::subtitle::{Cue, Subtitle};

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

/// Pack a [`Subtitle`] into a single fragmented `wvtt` track: `ftyp` · `moov` ·
/// `sidx` · one `styp` · `moof` · `mdat` per fragment. The `sidx` sits ahead of
/// the fragments and gives each one's size and duration, so a reader can index
/// the whole track from the head of the file.
///
/// The cues are tiled into samples that cover the timeline from 0 with no holes,
/// as the format requires: cues on screen together share one sample, and an
/// interval no cue covers becomes an empty sample. Samples are then grouped into
/// fragments — cutting at every splice point in `boundaries_ms`, and otherwise
/// once a fragment reaches `min_segment_length_ms` — which is the policy
/// `dyndo-core` applies when it groups fragments into segments, so these
/// fragments can be regrouped to line up with the asset's other tracks. A cue
/// crossing a cut is split, appearing in both fragments.
///
/// The track declares no language; that belongs to the transport.
///
/// # Errors
///
/// [`WvttError::Empty`] if no cue ends after time 0, since the result would be a
/// track with no fragments to index, and [`WvttError::Atom`] if a box fails to
/// encode.
pub fn pack(
    subtitle: &Subtitle,
    boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Result<Vec<u8>, WvttError> {
    let samples = tile(subtitle, boundaries_ms);
    let Some(duration_ms) = samples.last().map(|sample| sample.end_ms) else {
        return Err(WvttError::Empty);
    };

    let mut fragments = Vec::new();
    let mut references = Vec::new();
    for (index, group) in group(&samples, boundaries_ms, min_segment_length_ms)
        .into_iter()
        .enumerate()
    {
        let fragment = fragment(index, &samples[group.clone()])?;
        references.push(reference(fragment.len(), &samples[group]));
        fragments.push(fragment);
    }

    let mut track = Vec::new();
    ftyp().encode(&mut track)?;
    moov(duration_ms).encode(&mut track)?;
    Sidx {
        reference_id: TRACK_ID,
        timescale: TIMESCALE,
        earliest_presentation_time: 0,
        first_offset: 0,
        references,
    }
    .encode(&mut track)?;
    for fragment in fragments {
        track.extend_from_slice(&fragment);
    }

    Ok(track)
}

/// One sample: the cues on screen over `[start_ms, end_ms)`. No cues means a
/// gap, which the format still spends a sample on.
struct Sample<'a> {
    start_ms: u64,
    end_ms: u64,
    texts: Vec<&'a str>,
}

impl Sample<'_> {
    fn duration_ms(&self) -> u64 {
        self.end_ms - self.start_ms
    }

    fn write<B: BufMut>(&self, buf: &mut B) -> mp4_atom::Result<()> {
        if self.texts.is_empty() {
            return Vtte.encode(buf);
        }

        for text in &self.texts {
            Vttc {
                payl: Payl {
                    text: (*text).to_string(),
                },
            }
            .encode(buf)?;
        }

        Ok(())
    }
}

/// Cut the timeline at every cue edge and every splice point, then fill each
/// interval with the cues covering it.
fn tile<'a>(subtitle: &'a Subtitle, boundaries_ms: &[u64]) -> Vec<Sample<'a>> {
    let Some(track_end_ms) = subtitle.cues.iter().map(|cue| cue.end_ms).max() else {
        return Vec::new();
    };

    let mut edges = Vec::with_capacity(2 * subtitle.cues.len() + boundaries_ms.len() + 2);
    edges.push(0);
    edges.push(track_end_ms);
    for cue in &subtitle.cues {
        edges.push(cue.start_ms);
        edges.push(cue.end_ms);
    }
    edges.extend(
        boundaries_ms
            .iter()
            .copied()
            .filter(|&boundary_ms| boundary_ms < track_end_ms),
    );
    edges.sort_unstable();
    edges.dedup();

    let mut samples = Vec::with_capacity(edges.len() - 1);
    let mut active: Vec<&Cue> = Vec::new();
    let mut next = 0;
    for edge in edges.windows(2) {
        let (start_ms, end_ms) = (edge[0], edge[1]);
        while let Some(cue) = subtitle
            .cues
            .get(next)
            .filter(|cue| cue.start_ms <= start_ms)
        {
            active.push(cue);
            next += 1;
        }
        active.retain(|cue| cue.end_ms > start_ms);

        samples.push(Sample {
            start_ms,
            end_ms,
            // No cue edge falls inside the interval, so every still-active cue
            // spans all of it.
            texts: active.iter().map(|cue| cue.text.as_str()).collect(),
        });
    }

    samples
}

fn group(
    samples: &[Sample],
    boundaries_ms: &[u64],
    min_segment_length_ms: u64,
) -> Vec<Range<usize>> {
    let mut splices = boundaries_ms.to_vec();
    splices.sort_unstable();

    let mut groups = Vec::new();
    let mut start = 0;
    let mut duration_ms = 0;
    for (index, sample) in samples.iter().enumerate() {
        duration_ms += sample.duration_ms();
        let long_enough = duration_ms >= min_segment_length_ms;
        let at_splice = splices.binary_search(&sample.end_ms).is_ok();

        if long_enough || at_splice || index + 1 == samples.len() {
            groups.push(start..index + 1);
            start = index + 1;
            duration_ms = 0;
        }
    }

    groups
}

fn fragment(index: usize, samples: &[Sample]) -> Result<Vec<u8>, WvttError> {
    let mut data = Vec::new();
    let mut entries = Vec::with_capacity(samples.len());
    for sample in samples {
        let offset = data.len();
        sample.write(&mut data)?;
        entries.push(TrunEntry {
            duration: Some(milliseconds(sample.duration_ms())),
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
                base_media_decode_time: samples[0].start_ms,
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

fn reference(size: usize, samples: &[Sample]) -> SegmentReference {
    let duration_ms = samples.iter().map(Sample::duration_ms).sum();

    SegmentReference {
        reference_type: false,
        reference_size: u32::try_from(size).expect("a fragment fits in u32 bytes"),
        subsegment_duration: milliseconds(duration_ms),
        // Every text sample can be decoded on its own.
        starts_with_sap: true,
        sap_type: 1,
        sap_delta_time: 0,
    }
}

fn milliseconds(duration_ms: u64) -> u32 {
    u32::try_from(duration_ms).expect("a track is shorter than u32 milliseconds")
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

fn moov(duration_ms: u64) -> Moov {
    Moov {
        mvhd: Mvhd {
            creation_time: 0,
            modification_time: 0,
            timescale: TIMESCALE,
            duration: duration_ms,
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
                duration: duration_ms,
                enabled: true,
                in_movie: true,
                ..Tkhd::default()
            },
            mdia: Mdia {
                mdhd: Mdhd {
                    timescale: TIMESCALE,
                    duration: duration_ms,
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

/// `VTTCueBox`: one cue within a sample. Styling, positioning, and cue
/// identifiers are all optional boxes this crate does not model, leaving the
/// payload as the only child.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Vttc {
    payl: Payl,
}

impl Atom for Vttc {
    const KIND: FourCC = FourCC::new(b"vttc");

    fn decode_body<B: Buf>(buf: &mut B) -> mp4_atom::Result<Self> {
        Ok(Self {
            payl: Payl::decode(buf)?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> mp4_atom::Result<()> {
        self.payl.encode(buf)
    }
}

/// `CuePayloadBox`: the cue text, as UTF-8 filling the box.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Payl {
    text: String,
}

impl Atom for Payl {
    const KIND: FourCC = FourCC::new(b"payl");

    fn decode_body<B: Buf>(buf: &mut B) -> mp4_atom::Result<Self> {
        let size = buf.remaining();
        let text = String::from_utf8(buf.slice(size).to_vec())
            .map_err(|error| mp4_atom::Error::InvalidString(error.to_string()))?;
        buf.advance(size);

        Ok(Self { text })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> mp4_atom::Result<()> {
        self.text.as_bytes().encode(buf)
    }
}

/// `VTTEmptyCueBox`: a sample covering an interval with nothing on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Vtte;

impl Atom for Vtte {
    const KIND: FourCC = FourCC::new(b"vtte");

    fn decode_body<B: Buf>(_buf: &mut B) -> mp4_atom::Result<Self> {
        Ok(Self)
    }

    fn encode_body<B: BufMut>(&self, _buf: &mut B) -> mp4_atom::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mp4_atom::Decode;

    use super::*;

    #[test]
    fn packs_an_indexable_track() {
        let track = pack(&subtitle(&[(0, 1_000, "A")]), &[], 0).unwrap();

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
        let track = pack(&subtitle(&[(0, 1_000, "A")]), &[], 0).unwrap();
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
        let track = pack(&subtitle(&[(0, 1_000, "A"), (4_000, 6_500, "B")]), &[], 0).unwrap();
        let moov = moov_of(&track);

        assert_eq!(
            (moov.mvhd.duration, moov.trak[0].mdia.mdhd.duration),
            (6_500, 6_500)
        );
    }

    #[test]
    fn every_reference_matches_its_fragment() {
        let track = pack(&subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]), &[], 0).unwrap();
        let (sidx, fragments) = fragments_of(&track);

        assert_eq!(
            sidx.references
                .iter()
                .map(|reference| (reference.reference_size, reference.subsegment_duration))
                .collect::<Vec<_>>(),
            fragments
                .iter()
                .map(|fragment| (fragment.size, fragment.duration_ms))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn references_are_random_access_media() {
        let track = pack(&subtitle(&[(0, 1_000, "A")]), &[], 0).unwrap();
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
    fn tiles_gaps_between_cues() {
        let track = pack(&subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]), &[], 0).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert_eq!(
            fragments
                .iter()
                .map(|fragment| (fragment.start_ms, fragment.duration_ms))
                .collect::<Vec<_>>(),
            [(0, 1_000), (1_000, 1_000), (2_000, 1_000)]
        );
    }

    #[test]
    fn a_gap_becomes_an_empty_cue_sample() {
        let track = pack(&subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]), &[], 0).unwrap();
        let (_, fragments) = fragments_of(&track);

        let gap = &fragments[1].samples[0];
        assert_eq!(Vtte::decode(&mut gap.as_slice()).unwrap(), Vtte);
    }

    #[test]
    fn a_cue_sample_carries_its_text() {
        let track = pack(&subtitle(&[(0, 1_000, "Hello")]), &[], 0).unwrap();
        let (_, fragments) = fragments_of(&track);

        let sample = &fragments[0].samples[0];
        assert_eq!(
            Vttc::decode(&mut sample.as_slice()).unwrap(),
            Vttc {
                payl: Payl {
                    text: "Hello".to_string()
                }
            }
        );
    }

    #[test]
    fn simultaneous_cues_share_one_sample() {
        let track = pack(&subtitle(&[(0, 2_000, "A"), (1_000, 3_000, "B")]), &[], 0).unwrap();
        let (_, fragments) = fragments_of(&track);

        // [0,1000)=A, [1000,2000)=A+B, [2000,3000)=B
        let overlap = &mut fragments[1].samples[0].as_slice();
        assert_eq!(
            [
                Vttc::decode(overlap).unwrap().payl.text,
                Vttc::decode(overlap).unwrap().payl.text
            ],
            ["A".to_string(), "B".to_string()]
        );
    }

    #[test]
    fn a_zero_minimum_gives_every_sample_its_own_fragment() {
        let track = pack(&subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B")]), &[], 0).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert!(fragments.iter().all(|fragment| fragment.samples.len() == 1));
    }

    #[test]
    fn samples_are_grouped_until_the_minimum() {
        let cues = subtitle(&[(0, 1_000, "A"), (2_000, 3_000, "B"), (4_000, 5_000, "C")]);
        let track = pack(&cues, &[], 3_000).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert_eq!(
            fragments
                .iter()
                .map(|fragment| fragment.duration_ms)
                .collect::<Vec<_>>(),
            [3_000, 2_000]
        );
    }

    #[test]
    fn a_fragment_closes_at_a_splice_point() {
        let track = pack(&subtitle(&[(0, 6_000, "A")]), &[2_000], 10_000).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert_eq!(
            fragments
                .iter()
                .map(|fragment| (fragment.start_ms, fragment.duration_ms))
                .collect::<Vec<_>>(),
            [(0, 2_000), (2_000, 4_000)]
        );
    }

    #[test]
    fn a_cue_crossing_a_splice_point_appears_in_both_fragments() {
        let track = pack(&subtitle(&[(0, 6_000, "A")]), &[2_000], 10_000).unwrap();
        let (_, fragments) = fragments_of(&track);

        let texts: Vec<String> = fragments
            .iter()
            .map(|fragment| {
                Vttc::decode(&mut fragment.samples[0].as_slice())
                    .unwrap()
                    .payl
                    .text
            })
            .collect();
        assert_eq!(texts, ["A".to_string(), "A".to_string()]);
    }

    #[test]
    fn splice_points_outside_the_track_are_ignored() {
        let track = pack(&subtitle(&[(0, 2_000, "A")]), &[0, 2_000, 9_000], 10_000).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert_eq!(fragments.len(), 1);
    }

    #[test]
    fn samples_follow_the_mdat_header() {
        let track = pack(&subtitle(&[(0, 1_000, "A")]), &[], 0).unwrap();
        let (_, fragments) = fragments_of(&track);

        assert_eq!(
            fragments[0].data_offset,
            i32::try_from(fragments[0].moof_size + 8).unwrap()
        );
    }

    #[test]
    fn a_subtitle_without_cues_is_rejected() {
        let error = pack(&Subtitle::default(), &[], 0).unwrap_err();

        assert!(matches!(error, WvttError::Empty));
    }

    #[test]
    fn a_subtitle_ending_at_time_zero_is_rejected() {
        let error = pack(&subtitle(&[(0, 0, "A")]), &[], 0).unwrap_err();

        assert!(matches!(error, WvttError::Empty));
    }

    #[test]
    fn a_track_starting_after_time_zero_opens_with_a_gap() {
        let track = pack(&subtitle(&[(2_000, 3_000, "A")]), &[], 0).unwrap();
        let (_, fragments) = fragments_of(&track);

        let opening = &fragments[0];
        assert_eq!(
            (opening.start_ms, opening.duration_ms),
            (0, 2_000),
            "expected a gap covering the lead-in"
        );
        assert_eq!(
            Vtte::decode(&mut opening.samples[0].as_slice()).unwrap(),
            Vtte
        );
    }

    fn subtitle(cues: &[(u64, u64, &str)]) -> Subtitle {
        Subtitle {
            cues: cues
                .iter()
                .map(|&(start_ms, end_ms, text)| Cue {
                    start_ms,
                    end_ms,
                    text: text.to_string(),
                })
                .collect(),
        }
    }

    struct Fragment {
        start_ms: u64,
        duration_ms: u32,
        size: u32,
        moof_size: usize,
        data_offset: i32,
        samples: Vec<Vec<u8>>,
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
                    let sample = mdat.data[offset..offset + size].to_vec();
                    offset += size;
                    sample
                })
                .collect();

            fragments.push(Fragment {
                start_ms: traf.tfdt.as_ref().unwrap().base_media_decode_time,
                duration_ms: trun
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
