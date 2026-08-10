use mp4_atom::{Encode, Mdat, Mfhd, Moof, Tfdt, Tfhd, Traf, Trun, TrunEntry};

use super::format::Format;
use super::{MediaSegment, PackageError, Sample, UnpackageError};

pub(super) fn write<F: Format>(
    format: &F,
    track_id: u32,
    index: usize,
    segment: &MediaSegment<F::Payload>,
) -> Result<Vec<u8>, PackageError> {
    let mut data = Vec::new();
    let mut entries = Vec::with_capacity(segment.samples().len());
    for sample in segment.samples() {
        let offset = data.len();
        format.write_sample(sample.payload(), &mut data)?;
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
        .ok_or(PackageError::TooManyMediaSegments)?;
    let mut moof = Moof {
        mfhd: Mfhd { sequence_number },
        traf: vec![Traf {
            tfhd: Tfhd {
                track_id,
                default_base_is_moof: true,
                ..Tfhd::default()
            },
            tfdt: Some(Tfdt {
                base_media_decode_time: segment.base_decode_time(),
            }),
            trun: vec![Trun {
                data_offset: Some(0),
                entries,
            }],
            ..Traf::default()
        }],
    };

    let mut bytes = Vec::new();
    format.segment_type().encode(&mut bytes)?;
    let moof_start = bytes.len();
    moof.encode(&mut bytes)?;
    let data_offset = bytes
        .len()
        .checked_sub(moof_start)
        .and_then(|size| size.checked_add(8))
        .and_then(|size| i32::try_from(size).ok())
        .ok_or(PackageError::MediaSegmentTooLarge)?;
    moof.traf[0].trun[0].data_offset = Some(data_offset);
    bytes.truncate(moof_start);
    moof.encode(&mut bytes)?;
    Mdat { data }.encode(&mut bytes)?;

    Ok(bytes)
}

pub(super) fn read<F: Format>(
    format: &F,
    header: &Moof,
    data: &[u8],
) -> Result<MediaSegment<F::Payload>, UnpackageError> {
    let mut base_decode_time = None;
    let mut samples = Vec::new();

    for traf in &header.traf {
        let tfdt = traf.tfdt.as_ref().ok_or(UnpackageError::MissingBaseTime)?;
        base_decode_time.get_or_insert(tfdt.base_media_decode_time);
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
            samples.push(Sample::new(duration, format.read_sample(bytes)?));
            offset = end;
        }
    }

    Ok(MediaSegment::new(base_decode_time.unwrap_or(0), samples))
}
