//! One fragment of a `wvtt` track: `styp` · `moof` · `mdat`, with the samples tiling
//! it packed into the `mdat`.
//!
//! [`encode`] and [`decode`] are inverses.

use mp4_atom::{Encode, FourCC, Mdat, Mfhd, Moof, Styp, Tfdt, Tfhd, Traf, Trun, TrunEntry};

use super::{PackError, TRACK_ID, UnpackError, sample};
use crate::fragmenter::{Fragment, Sample};

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
        sample::encode(sample, &mut data)?;
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
                cues: sample::decode(bytes, start, stop)?,
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
