use mp4_atom::{Encode, Sidx};

use super::format::Format;
use super::{MediaSegment, PackageError, initialization};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Packager<F> {
    format: F,
    track_id: u32,
    timescale: u32,
}

impl<F> Packager<F> {
    pub(crate) fn new(format: F, timescale: u32) -> Self {
        Self {
            format,
            track_id: 1,
            timescale,
        }
    }

    pub(crate) fn with_track_id(mut self, track_id: u32) -> Self {
        self.track_id = track_id;
        self
    }

    pub fn track_id(&self) -> u32 {
        self.track_id
    }

    pub fn timescale(&self) -> u32 {
        self.timescale
    }
}

impl<F: Format> Packager<F> {
    pub(crate) fn package(
        &self,
        segments: &[MediaSegment<F::Payload>],
    ) -> Result<Vec<u8>, PackageError> {
        if self.track_id == 0 {
            return Err(PackageError::InvalidTrackId);
        }
        if self.timescale == 0 {
            return Err(PackageError::InvalidTimescale);
        }

        let duration = segments
            .last()
            .map(|segment| {
                segment
                    .base_decode_time()
                    .saturating_add(segment.duration())
            })
            .unwrap_or(0);
        if duration == 0 {
            return Err(PackageError::Empty);
        }

        let mut serialized_segments = Vec::with_capacity(segments.len());
        let mut references = Vec::with_capacity(segments.len());
        for (index, segment) in segments.iter().enumerate() {
            let sequence_number = index
                .checked_add(1)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(PackageError::TooManyMediaSegments)?;
            let bytes = segment.serialize(&self.format, self.track_id, sequence_number)?;
            let size =
                u32::try_from(bytes.len()).map_err(|_| PackageError::MediaSegmentTooLarge)?;
            references.push(initialization::reference(size, segment)?);
            serialized_segments.push(bytes);
        }

        let mut bytes = Vec::new();
        self.format.file_type().encode(&mut bytes)?;
        initialization::movie(&self.format, self.track_id, self.timescale, duration)
            .encode(&mut bytes)?;
        Sidx {
            reference_id: self.track_id,
            timescale: self.timescale,
            earliest_presentation_time: segments
                .first()
                .map_or(0, |segment| segment.base_decode_time()),
            first_offset: 0,
            references,
        }
        .encode(&mut bytes)?;
        for segment in serialized_segments {
            bytes.extend_from_slice(&segment);
        }

        Ok(bytes)
    }
}
