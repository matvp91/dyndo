//! One fragment of a `wvtt` track: `styp` · `moof` · `mdat`, and the samples the
//! `mdat` holds.
//!
//! [`encode`] and [`decode`] are inverses, as are the sample-level pair beneath
//! them. A sample is not a level of its own here: the `mdat` *is* the samples, and
//! the `trun` that says where each one ends is the same fragment's business.

use mp4_atom::{
    Any, Atom, BufMut, DecodeMaybe, Encode, FourCC, Mdat, Mfhd, Moof, Styp, Tfdt, Tfhd, Traf, Trun,
    TrunEntry,
};

use super::atom::{Payl, Vttc, Vtte};
use super::{PackError, TRACK_ID, UnpackError};
use crate::fragmenter::{Fragment, Sample};
use crate::subtitle::Cue;

/// Write one fragment, its `trun` listing each sample's duration and size so a
/// reader can cut the `mdat` back up. `index` numbers the fragment within its track.
///
/// # Errors
///
/// [`PackError::Atom`] if a box fails to encode.
pub(super) fn encode(index: usize, fragment: &Fragment) -> Result<Vec<u8>, PackError> {
    let mut data = Vec::new();
    let mut entries = Vec::with_capacity(fragment.samples.len());
    for sample in &fragment.samples {
        let offset = data.len();
        write_sample(sample, &mut data)?;
        entries.push(TrunEntry {
            duration: Some(sample.duration()),
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
                base_media_decode_time: u64::from(fragment.start),
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

/// One fragment, its samples timed from the base decode time its `tfdt` carries and
/// cut from `data` by the sizes its `trun` lists. The fragment spans the samples it
/// holds, which tile it without holes.
///
/// # Errors
///
/// [`UnpackError`] if the fragment carries no base decode time or sample durations,
/// if the sample sizes overrun `data`, or if a time does not fit the milliseconds a
/// sample counts.
pub(super) fn decode(header: &Moof, data: &[u8], timescale: u32) -> Result<Fragment, UnpackError> {
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
            let bytes = data.get(offset..end).ok_or(UnpackError::SampleOutOfRange)?;
            let next = time
                .checked_add(u64::from(duration))
                .ok_or(UnpackError::TimeOverflow(time))?;

            let start = milliseconds(time, timescale)?;
            let stop = milliseconds(next, timescale)?;
            samples.push(Sample {
                start,
                end: stop,
                cues: read_sample(bytes, start, stop)?,
            });

            offset = end;
            time = next;
        }
    }

    Ok(Fragment {
        start: samples.first().map_or(0, |sample| sample.start),
        end: samples.last().map_or(0, |sample| sample.end),
        samples,
    })
}

/// Write the cues on screen over a sample, each a `vttc` carrying its text. An
/// interval showing nothing is a lone `vtte`, which the format still spends a sample
/// on.
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

/// The cues one sample carries, one per `vttc` on screen over it. A `vtte` carries
/// none.
///
/// Each cue spans the sample, since a `vttc` records what is on screen without saying
/// for how long. The authored span is recoverable only by merging the samples a cue
/// runs across, which [`merge`](crate::fragmenter::merge) does.
fn read_sample(bytes: &[u8], start: u32, end: u32) -> Result<Vec<Cue>, UnpackError> {
    let mut cues = Vec::new();
    let mut buf = bytes;

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

/// A media time in the milliseconds a [`Sample`] counts. The timescale comes from
/// the probe, which rejects a zero one.
fn milliseconds(time: u64, timescale: u32) -> Result<u32, UnpackError> {
    let millis = u128::from(time) * 1_000 / u128::from(timescale);

    u32::try_from(millis).map_err(|_| UnpackError::TimeOverflow(time))
}
