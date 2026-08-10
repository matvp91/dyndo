use mp4_atom::{Encode, Mdat, Mfhd, Moof, Tfdt, Tfhd, Traf, Trun, TrunEntry};

use super::format::Format;
use super::{PackageError, UnpackageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSegment<S> {
    base_decode_time: u64,
    samples: Vec<Sample<S>>,
}

impl<S> MediaSegment<S> {
    pub fn new(base_decode_time: u64, samples: Vec<Sample<S>>) -> Self {
        Self {
            base_decode_time,
            samples,
        }
    }

    pub fn base_decode_time(&self) -> u64 {
        self.base_decode_time
    }

    pub fn duration(&self) -> u64 {
        self.samples
            .iter()
            .map(|sample| u64::from(sample.duration))
            .sum()
    }

    pub fn samples(&self) -> &[Sample<S>] {
        &self.samples
    }

    pub(super) fn serialize<F: Format<Payload = S>>(
        &self,
        format: &F,
        track_id: u32,
        sequence_number: u32,
    ) -> Result<Vec<u8>, PackageError> {
        let mut data = Vec::new();
        let mut entries = Vec::with_capacity(self.samples.len());
        for sample in &self.samples {
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

        let mut moof = Moof {
            mfhd: Mfhd { sequence_number },
            traf: vec![Traf {
                tfhd: Tfhd {
                    track_id,
                    default_base_is_moof: true,
                    ..Tfhd::default()
                },
                tfdt: Some(Tfdt {
                    base_media_decode_time: self.base_decode_time,
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

    pub(super) fn parse<F: Format<Payload = S>>(
        format: &F,
        header: &Moof,
        data: &[u8],
    ) -> Result<Self, UnpackageError> {
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

        Ok(Self::new(base_decode_time.unwrap_or(0), samples))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample<S> {
    duration: u32,
    payload: S,
}

impl<S> Sample<S> {
    pub fn new(duration: u32, payload: S) -> Self {
        Self { duration, payload }
    }

    pub fn duration(&self) -> u32 {
        self.duration
    }

    pub fn payload(&self) -> &S {
        &self.payload
    }
}
