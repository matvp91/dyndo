//! Reading a served segment of a CMAF `wvtt` track back into its cues.

use mp4_atom::{Any, Atom, DecodeMaybe, Moof};

use super::UnpackError;
use crate::atoms::Vttc;
use crate::fragmenter::{Fragment, Sample};
use crate::subtitle::Cue;

/// Unpack one served segment of a `wvtt` track into the fragments it carries.
///
/// `segment` holds the whole byte range a segment resolves to: one or more
/// `styp` · `moof` · `mdat` triples, since a segment groups several fragments once
/// a minimum length asks it to. `timescale` is the track's, as probed — only a
/// track this crate packed is guaranteed to count in milliseconds.
///
/// The result stops at the samples: pass it to
/// [`merge`](crate::fragmenter::merge) for the cues they carry. This reads back
/// what [`pack`](crate::muxer::wvtt::pack) wrote, and no more. A `wvtt` track from
/// another packager may carry cue settings, identifiers and styling that a sample
/// has nowhere to hold, and those boxes are ignored rather than reported.
///
/// # Errors
///
/// [`UnpackError`] if a box fails to decode, if a fragment carries no base decode
/// time or sample durations, if the fragment headers and sample data do not pair
/// up, if the sample sizes overrun their `mdat`, or if a time does not fit the
/// milliseconds a sample counts.
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
                fragments.push(read_fragment(&header, &mdat.data, timescale)?);
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

/// One fragment, its samples timed from the base decode time its `tfdt` carries and
/// cut from `data` by the sizes its `trun` lists. The fragment spans the samples it
/// holds, which tile it without holes.
fn read_fragment(header: &Moof, data: &[u8], timescale: u32) -> Result<Fragment, UnpackError> {
    let mut samples: Vec<Sample> = Vec::new();

    for traf in &header.traf {
        let tfdt = traf.tfdt.as_ref().ok_or(UnpackError::MissingBaseTime)?;
        let mut time = tfdt.base_media_decode_time;
        // Samples run contiguously from the start of the sample data, which is
        // what the `trun`'s moof-anchored data offset also says.
        let mut offset = 0usize;

        for entry in traf.trun.iter().flat_map(|trun| &trun.entries) {
            let duration = entry.duration.ok_or(UnpackError::MissingSampleTiming)?;
            let size = entry.size.ok_or(UnpackError::MissingSampleTiming)?;
            let end = offset
                .checked_add(size as usize)
                .ok_or(UnpackError::SampleOutOfRange)?;
            let sample = data.get(offset..end).ok_or(UnpackError::SampleOutOfRange)?;
            let next = time
                .checked_add(u64::from(duration))
                .ok_or(UnpackError::TimeOverflow(time))?;

            let start = milliseconds(time, timescale)?;
            let end_time = milliseconds(next, timescale)?;
            samples.push(Sample {
                start,
                end: end_time,
                cues: cues(sample, start, end_time)?,
            });

            offset = end;
            time = next;
        }
    }

    let start = samples.first().map_or(0, |sample| sample.start);
    let end = samples.last().map_or(0, |sample| sample.end);

    Ok(Fragment {
        start,
        end,
        samples,
    })
}

/// The cues a sample carries, one per `vttc` on screen over it. A `vtte` carries
/// none — the box the format spends on an interval showing nothing.
///
/// Each cue spans the sample, since a `vttc` records what is on screen without
/// saying for how long: the authored span is only recoverable by merging the samples
/// a cue runs across.
fn cues(sample: &[u8], start: u32, end: u32) -> Result<Vec<Cue>, UnpackError> {
    let mut cues = Vec::new();
    let mut buf = sample;

    while let Some(atom) = Any::decode_maybe(&mut buf)? {
        // The cue boxes are ours rather than mp4-atom's, so they arrive unknown.
        let Any::Unknown(kind, body) = atom else {
            continue;
        };
        if kind == Vttc::KIND {
            let vttc = Vttc::decode_body(&mut body.as_slice())?;
            cues.push(Cue {
                start,
                end,
                text: vttc.payl.text,
            });
        }
    }

    Ok(cues)
}

/// A media time in the milliseconds a [`Cue`] counts. The timescale comes from the
/// probe, which rejects a zero one.
fn milliseconds(time: u64, timescale: u32) -> Result<u32, UnpackError> {
    let millis = u128::from(time) * 1_000 / u128::from(timescale);

    u32::try_from(millis).map_err(|_| UnpackError::TimeOverflow(time))
}

#[cfg(test)]
mod tests {
    use mp4_atom::{Encode, Mdat, Mfhd, Tfdt, Tfhd, Traf, Trun, TrunEntry};

    use super::*;
    use crate::subtitle::Subtitle;
    use crate::{fragmenter, muxer};

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

        assert_eq!(crate::vtt::parse(&unpacked.write()).unwrap(), authored);
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
        fragmenter::merge(&unpack(segment, timescale).unwrap())
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
        let fragments = fragmenter::fragment(subtitle, boundaries, length);
        let packed = muxer::wvtt::pack(&fragments).unwrap();

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
