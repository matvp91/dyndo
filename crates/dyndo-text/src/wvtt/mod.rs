//! The CMAF `wvtt` container (ISO/IEC 14496-30): fragments in, bytes out, and back.
//!
//! Filed by level rather than by direction, because the two directions do not meet
//! at the same one. [`pack`] writes a whole track — `ftyp`, `moov`, `sidx` and every
//! fragment — while [`unpack`] reads one served segment, which is fragments and
//! nothing else. Where they do mirror each other is further down, at a fragment and
//! at a sample, and each of those modules holds both halves of its pair.
//!
//! Nothing here knows what a [`Subtitle`](crate::subtitle::Subtitle) is. Cues arrive
//! already divided into samples and leave the same way; turning samples back into a
//! subtitle is [`merge`](crate::fragmenter::merge)'s job.

mod atom;
mod fragment;
mod sample;
mod track;

use mp4_atom::{Any, DecodeMaybe, Encode, Moof, Sidx};

use crate::fragmenter::Fragment;

/// Milliseconds map 1:1 onto media time.
const TIMESCALE: u32 = 1_000;

const TRACK_ID: u32 = 1;

/// What stops fragments from being written out as a track.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("subtitle covers no time")]
    Empty,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}

/// What stops a served segment from being read back.
#[derive(Debug, thiserror::Error)]
pub enum UnpackError {
    #[error("fragment carries no base decode time")]
    MissingBaseTime,
    #[error("fragment carries no sample durations")]
    MissingSampleTiming,
    #[error("sample data overruns the fragment")]
    SampleOutOfRange,
    #[error("a fragment header and its sample data do not pair up")]
    UnpairedFragment,
    #[error("time {0} does not fit in the milliseconds a cue counts")]
    TimeOverflow(u64),
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
/// [`PackError::Empty`] if the fragments run to time 0, since the result would be
/// a track with nothing to index, and [`PackError::Atom`] if a box fails to
/// encode.
pub fn pack(fragments: &[Fragment]) -> Result<Vec<u8>, PackError> {
    let track_end = fragments.last().map_or(0, |fragment| fragment.end);
    if track_end == 0 {
        return Err(PackError::Empty);
    }

    let mut encoded = Vec::with_capacity(fragments.len());
    let mut references = Vec::with_capacity(fragments.len());
    for (index, fragment) in fragments.iter().enumerate() {
        let bytes = fragment::encode(index, fragment)?;
        references.push(track::reference(bytes.len(), fragment));
        encoded.push(bytes);
    }

    let mut track = Vec::new();
    track::ftyp().encode(&mut track)?;
    track::moov(u64::from(track_end)).encode(&mut track)?;
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

/// Unpack one served segment of a `wvtt` track into the fragments it carries.
///
/// `segment` holds the whole byte range a segment resolves to: one or more
/// `styp` · `moof` · `mdat` triples, since a segment groups several fragments once
/// a minimum length asks it to. `timescale` is the track's, as probed — only a
/// track this crate packed is guaranteed to count in milliseconds.
///
/// The result stops at the samples: pass it to
/// [`merge`](crate::fragmenter::merge) for the cues they carry. This reads back
/// what [`pack`] wrote, and no more. A `wvtt` track from another packager may carry
/// cue settings, identifiers and styling that a cue has nowhere to hold, and those
/// boxes are ignored rather than reported.
///
/// # Errors
///
/// [`UnpackError`] if a box fails to decode, if a fragment carries no base decode
/// time or sample durations, if the fragment headers and sample data do not pair
/// up, if the sample sizes overrun their `mdat`, or if a time does not fit the
/// milliseconds a cue counts.
pub fn unpack(segment: &[u8], timescale: u32) -> Result<Vec<Fragment>, UnpackError> {
    let mut fragments = Vec::new();
    let mut header: Option<Moof> = None;
    let mut buf = segment;

    while let Some(atom) = Any::decode_maybe(&mut buf)? {
        match atom {
            Any::Moof(moof) => {
                if header.replace(moof).is_some() {
                    return Err(UnpackError::UnpairedFragment);
                }
            }
            Any::Mdat(mdat) => {
                let header = header.take().ok_or(UnpackError::UnpairedFragment)?;
                fragments.push(fragment::decode(&header, &mdat.data, timescale)?);
            }
            _ => {}
        }
    }

    // A truncated read ends the loop rather than failing to decode, so a fragment
    // header left without its data is how that surfaces.
    if header.is_some() {
        return Err(UnpackError::UnpairedFragment);
    }

    Ok(fragments)
}

#[cfg(test)]
mod tests {
    use mp4_atom::{
        Buf, Codec, Decode, FourCC, Ftyp, Mdat, Mfhd, Moov, Styp, Tfdt, Tfhd, Traf, Trun, TrunEntry,
    };

    use super::atom::{Payl, Vttc, Vtte};
    use super::*;
    use crate::fragmenter::{fragment, merge};
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

        assert!(matches!(error, PackError::Empty));
    }

    #[test]
    fn a_subtitle_ending_at_time_zero_is_rejected() {
        let subtitle = subtitle(&[(0, 0, "A")]);
        let error = pack(&fragment(&subtitle, &[], 0)).unwrap_err();

        assert!(matches!(error, PackError::Empty));
    }

    fn subtitle(cues: &[(u32, u32, &str)]) -> Subtitle {
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

    #[test]
    fn unpacks_a_single_cue() {
        let subtitle = subtitle(&[(0, 2_000, "Hello")]);

        let unpacked = merged(&segment(&subtitle, &[], 0), 1_000);

        assert_eq!(unpacked, subtitle);
    }

    #[test]
    fn merges_a_cue_the_samples_split() {
        // "B" coming and going cuts the fragment into three samples, and "A" is on
        // screen for all of them.
        let subtitle = subtitle(&[(0, 6_000, "A"), (1_000, 3_000, "B")]);

        let unpacked = merged(&segment(&subtitle, &[], 0), 1_000);

        assert_eq!(unpacked, subtitle);
    }

    #[test]
    fn merges_a_cue_across_the_fragments_of_one_segment() {
        let subtitle = subtitle(&[(0, 6_000, "A")]);

        let unpacked = merged(&segment(&subtitle, &[], 2_000), 1_000);

        assert_eq!(unpacked, subtitle);
    }

    #[test]
    fn keeps_cues_sharing_a_start_apart() {
        let subtitle = subtitle(&[(1_000, 2_000, "short"), (1_000, 4_000, "long")]);

        let unpacked = merged(&segment(&subtitle, &[], 0), 1_000);

        assert_eq!(unpacked, subtitle);
    }

    #[test]
    fn reads_an_interval_showing_nothing_as_no_cue() {
        let subtitle = subtitle(&[(2_000, 3_000, "A")]);

        // The track opens with an empty sample covering [0, 2000).
        let unpacked = merged(&segment(&subtitle, &[], 0), 1_000);

        assert_eq!(unpacked, subtitle);
    }

    #[test]
    fn merges_two_cues_carrying_the_same_text_back_to_back() {
        let authored = subtitle(&[(0, 1_000, "same"), (1_000, 2_000, "same")]);

        let unpacked = merged(&segment(&authored, &[], 0), 1_000);

        assert_eq!(unpacked, subtitle(&[(0, 2_000, "same")]));
    }

    #[test]
    fn converts_media_time_in_another_timescale() {
        let authored = subtitle(&[(0, 2_000, "Hello")]);

        let unpacked = merged(&segment(&authored, &[], 0), 500);

        assert_eq!(unpacked, subtitle(&[(0, 4_000, "Hello")]));
    }

    /// The invariant the whole feature rests on: what a `.vtt` request serves is
    /// the document the `.m4s` request's bytes were packed from.
    #[test]
    fn a_document_survives_the_round_trip_through_a_packed_track() {
        let document = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/text_sample.vtt"
        ))
        .unwrap();
        let authored = crate::vtt::parse(&document).unwrap();

        let unpacked = merged(&segment(&authored, &[7_400], 4_000), 1_000);

        assert_eq!(
            crate::vtt::parse(&crate::vtt::write(&unpacked)).unwrap(),
            authored
        );
    }

    #[test]
    fn rejects_a_truncated_segment() {
        let subtitle = subtitle(&[(0, 2_000, "Hello")]);
        let mut truncated = segment(&subtitle, &[], 0);
        truncated.truncate(truncated.len() - 4);

        let error = unpack(&truncated, 1_000).unwrap_err();

        assert!(matches!(error, UnpackError::UnpairedFragment));
    }

    #[test]
    fn rejects_a_fragment_without_a_base_decode_time() {
        let fragment = handmade(None, &[(Some(1_000), Some(0))], &[]);

        let error = unpack(&fragment, 1_000).unwrap_err();

        assert!(matches!(error, UnpackError::MissingBaseTime));
    }

    #[test]
    fn rejects_a_sample_without_a_duration() {
        let fragment = handmade(Some(0), &[(None, Some(0))], &[]);

        let error = unpack(&fragment, 1_000).unwrap_err();

        assert!(matches!(error, UnpackError::MissingSampleTiming));
    }

    #[test]
    fn rejects_samples_that_overrun_their_data() {
        let fragment = handmade(Some(0), &[(Some(1_000), Some(64))], &[0; 4]);

        let error = unpack(&fragment, 1_000).unwrap_err();

        assert!(matches!(error, UnpackError::SampleOutOfRange));
    }

    #[test]
    fn rejects_a_time_past_the_milliseconds_a_cue_counts() {
        let fragment = handmade(Some(5_000_000_000), &[(Some(0), Some(0))], &[]);

        let error = unpack(&fragment, 1_000).unwrap_err();

        assert!(matches!(error, UnpackError::TimeOverflow(5_000_000_000)));
    }

    /// The cues a segment carries: what a caller asks `unpack` for, merged.
    fn merged(segment: &[u8], timescale: u32) -> Subtitle {
        merge(&unpack(segment, timescale).unwrap())
    }

    /// A fragment built by hand, for the shapes the packer never writes. Each entry
    /// is a sample's `(duration, size)`.
    fn handmade(base: Option<u64>, entries: &[(Option<u32>, Option<u32>)], data: &[u8]) -> Vec<u8> {
        let header = Moof {
            mfhd: Mfhd { sequence_number: 1 },
            traf: vec![Traf {
                tfhd: Tfhd {
                    track_id: 1,
                    default_base_is_moof: true,
                    ..Tfhd::default()
                },
                tfdt: base.map(|base_media_decode_time| Tfdt {
                    base_media_decode_time,
                }),
                trun: vec![Trun {
                    data_offset: Some(0),
                    entries: entries
                        .iter()
                        .map(|&(duration, size)| TrunEntry {
                            duration,
                            size,
                            ..TrunEntry::default()
                        })
                        .collect(),
                }],
                ..Traf::default()
            }],
        };

        let mut bytes = Vec::new();
        header.encode(&mut bytes).unwrap();
        Mdat {
            data: data.to_vec(),
        }
        .encode(&mut bytes)
        .unwrap();

        bytes
    }

    /// The bytes a segment covering the whole track resolves to: everything past
    /// the `sidx`, which is where the first fragment's `styp` begins.
    fn segment(subtitle: &Subtitle, boundaries: &[u32], length: u32) -> Vec<u8> {
        let packed = pack(&fragment(subtitle, boundaries, length)).unwrap();

        let mut buf = packed.as_slice();
        loop {
            let offset = packed.len() - buf.len();
            match Any::decode_maybe(&mut buf).unwrap() {
                Some(Any::Styp(_)) => return packed[offset..].to_vec(),
                Some(_) => continue,
                None => panic!("packed track carries no fragments"),
            }
        }
    }
}
