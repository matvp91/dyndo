mod syntax;

use syntax::Context;

use super::Error;
use super::sample::{Subsample, SubsampleOrganizer};

pub(super) struct AvcSubsampleMapper {
    nal_length_size: usize,
    context: Context,
}

impl AvcSubsampleMapper {
    pub(super) fn new(
        nal_length_size: u8,
        sequence_parameter_sets: &[Vec<u8>],
        picture_parameter_sets: &[Vec<u8>],
    ) -> Result<Self, Error> {
        let nal_length_size = usize::from(nal_length_size);
        if !(1..=4).contains(&nal_length_size) {
            return Err(Error::InvalidMedia);
        }
        Ok(Self {
            nal_length_size,
            context: Context::new(sequence_parameter_sets, picture_parameter_sets)
                .map_err(|()| Error::InvalidMedia)?,
        })
    }

    pub(super) fn map(&mut self, sample: &[u8]) -> Result<Vec<Subsample>, Error> {
        let mut organizer = SubsampleOrganizer::default();
        let mut offset = 0_usize;
        while offset < sample.len() {
            let length_end = offset
                .checked_add(self.nal_length_size)
                .ok_or(Error::TooLarge)?;
            let encoded_length = sample.get(offset..length_end).ok_or(Error::InvalidMedia)?;
            let nal_size = encoded_length
                .iter()
                .fold(0_usize, |size, byte| (size << 8) | usize::from(*byte));
            if nal_size == 0 {
                return Err(Error::InvalidMedia);
            }
            let nal_end = length_end
                .checked_add(nal_size)
                .filter(|end| *end <= sample.len())
                .ok_or(Error::InvalidMedia)?;
            let nal = &sample[length_end..nal_end];
            let nal_type = nal[0] & 0x1f;
            let clear = if matches!(nal_type, 1 | 5) {
                self.nal_length_size
                    .checked_add(
                        self.context
                            .slice_header_size(nal)
                            .map_err(|()| Error::InvalidMedia)?,
                    )
                    .ok_or(Error::TooLarge)?
            } else {
                self.nal_length_size
                    .checked_add(nal_size)
                    .ok_or(Error::TooLarge)?
            };
            let encrypted = self
                .nal_length_size
                .checked_add(nal_size)
                .and_then(|total| total.checked_sub(clear))
                .ok_or(Error::InvalidMedia)?;
            organizer.add(clear, encrypted)?;
            offset = nal_end;
        }

        organizer.finish()
    }
}
