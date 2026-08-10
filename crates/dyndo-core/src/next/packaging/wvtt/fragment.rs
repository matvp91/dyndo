use mp4_atom::{
    Any, Atom, BufMut, DecodeMaybe, Encode, FourCC, Mdat, Mfhd, Moof, Styp, Tfdt, Tfhd, Traf, Trun,
    TrunEntry,
};

use super::super::{TimedFragment, TimedSample};
use super::atom::{Payl, Vttc, Vtte};
use super::{PackageError, TRACK_ID, UnpackageError, WvttSample};

pub(super) fn encode(
    index: usize,
    fragment: &TimedFragment<WvttSample>,
) -> Result<Vec<u8>, PackageError> {
    let mut data = Vec::new();
    let mut entries = Vec::with_capacity(fragment.samples().len());
    for sample in fragment.samples() {
        let offset = data.len();
        write_sample(sample.value(), &mut data)?;
        entries.push(TrunEntry {
            duration: Some(sample.duration()),
            size: Some(
                u32::try_from(data.len() - offset).map_err(|_| PackageError::SampleTooLarge)?,
            ),
            ..TrunEntry::default()
        });
    }

    let sequence_number = index
        .checked_add(1)
        .and_then(|index| u32::try_from(index).ok())
        .ok_or(PackageError::TooManyFragments)?;
    let mut moof = Moof {
        mfhd: Mfhd { sequence_number },
        traf: vec![Traf {
            tfhd: Tfhd {
                track_id: TRACK_ID,
                default_base_is_moof: true,
                ..Tfhd::default()
            },
            tfdt: Some(Tfdt {
                base_media_decode_time: fragment.start_time(),
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
    let moof_start = bytes.len();
    moof.encode(&mut bytes)?;
    let data_offset = bytes
        .len()
        .checked_sub(moof_start)
        .and_then(|size| size.checked_add(8))
        .and_then(|size| i32::try_from(size).ok())
        .ok_or(PackageError::FragmentTooLarge)?;
    moof.traf[0].trun[0].data_offset = Some(data_offset);
    bytes.truncate(moof_start);
    moof.encode(&mut bytes)?;
    Mdat { data }.encode(&mut bytes)?;

    Ok(bytes)
}

pub(super) fn decode(
    header: &Moof,
    data: &[u8],
) -> Result<TimedFragment<WvttSample>, UnpackageError> {
    let mut start_time = None;
    let mut samples = Vec::new();

    for traf in &header.traf {
        let tfdt = traf.tfdt.as_ref().ok_or(UnpackageError::MissingBaseTime)?;
        start_time.get_or_insert(tfdt.base_media_decode_time);
        let mut offset = 0usize;

        for entry in traf.trun.iter().flat_map(|trun| &trun.entries) {
            let duration = entry.duration.ok_or(UnpackageError::MissingSampleTiming)?;
            let size = entry.size.ok_or(UnpackageError::MissingSampleTiming)?;
            let end = offset
                .checked_add(size as usize)
                .ok_or(UnpackageError::SampleOutOfRange)?;
            let bytes = data
                .get(offset..end)
                .ok_or(UnpackageError::SampleOutOfRange)?;
            samples.push(TimedSample::new(duration, read_sample(bytes)?));
            offset = end;
        }
    }

    Ok(TimedFragment::new(start_time.unwrap_or(0), samples))
}

fn write_sample<B: BufMut>(sample: &WvttSample, buf: &mut B) -> mp4_atom::Result<()> {
    if sample.cues().is_empty() {
        return Vtte.encode(buf);
    }

    for cue in sample.cues() {
        Vttc {
            payl: Payl { text: cue.clone() },
        }
        .encode(buf)?;
    }

    Ok(())
}

fn read_sample(bytes: &[u8]) -> Result<WvttSample, UnpackageError> {
    let mut cues = Vec::new();
    let mut buf = bytes;

    while let Some(atom) = Any::decode_maybe(&mut buf)? {
        let Any::Unknown(kind, body) = atom else {
            continue;
        };
        if kind == Vttc::KIND {
            cues.push(Vttc::decode_body(&mut body.as_slice())?.payl.text);
        }
    }

    Ok(WvttSample::new(cues))
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
