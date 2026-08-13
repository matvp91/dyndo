use aes::Aes128;
use ctr::cipher::{KeyIvInit, StreamCipher};

use super::Error;
use super::avc::AvcSubsampleMapper;
use crate::codec::CodecConfig;

type Aes128Ctr = ctr::Ctr128BE<Aes128>;

pub(super) struct SampleEncryptionInfo {
    pub(super) iv: [u8; 8],
    pub(super) subsamples: Vec<Subsample>,
}

pub(super) struct Subsample {
    pub(super) clear: u16,
    pub(super) encrypted: u32,
}

pub(super) struct SampleEncryptor {
    key: [u8; 16],
    mapper: SampleMapper,
}

enum SampleMapper {
    FullSample,
    Avc(AvcSubsampleMapper),
}

impl SampleEncryptor {
    pub(super) fn new(key: [u8; 16], codec: &CodecConfig) -> Result<Self, Error> {
        let mapper = match codec {
            CodecConfig::Aac(_) => SampleMapper::FullSample,
            CodecConfig::Avc(codec) => SampleMapper::Avc(AvcSubsampleMapper::new(
                codec.nal_length_size(),
                codec.sequence_parameter_sets(),
                codec.picture_parameter_sets(),
            )?),
            _ => return Err(Error::UnsupportedCodec),
        };
        Ok(Self { key, mapper })
    }

    pub(super) fn uses_subsamples(&self) -> bool {
        matches!(self.mapper, SampleMapper::Avc(_))
    }

    pub(super) fn encrypt(
        &mut self,
        sample: &mut [u8],
        iv: [u8; 8],
    ) -> Result<SampleEncryptionInfo, Error> {
        let subsamples = match &mut self.mapper {
            SampleMapper::FullSample => Vec::new(),
            SampleMapper::Avc(mapper) => mapper.map(sample)?,
        };

        let mut counter = [0; 16];
        counter[..8].copy_from_slice(&iv);
        let mut cipher = Aes128Ctr::new(&self.key.into(), &counter.into());
        if subsamples.is_empty() {
            cipher.apply_keystream(sample);
        } else {
            encrypt_subsamples(sample, &subsamples, &mut cipher)?;
        }
        Ok(SampleEncryptionInfo { iv, subsamples })
    }
}

fn encrypt_subsamples(
    sample: &mut [u8],
    subsamples: &[Subsample],
    cipher: &mut Aes128Ctr,
) -> Result<(), Error> {
    let mut offset = 0_usize;
    for subsample in subsamples {
        offset = offset
            .checked_add(usize::from(subsample.clear))
            .ok_or(Error::TooLarge)?;
        let encrypted_end = offset
            .checked_add(subsample.encrypted as usize)
            .filter(|end| *end <= sample.len())
            .ok_or(Error::InvalidMedia)?;
        cipher.apply_keystream(&mut sample[offset..encrypted_end]);
        offset = encrypted_end;
    }
    if offset != sample.len() {
        return Err(Error::InvalidMedia);
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct SubsampleOrganizer {
    subsamples: Vec<Subsample>,
    clear: usize,
}

impl SubsampleOrganizer {
    pub(super) fn add(&mut self, mut clear: usize, mut encrypted: usize) -> Result<(), Error> {
        let misalignment = encrypted % 16;
        clear = clear.checked_add(misalignment).ok_or(Error::TooLarge)?;
        encrypted -= misalignment;
        self.clear = self.clear.checked_add(clear).ok_or(Error::TooLarge)?;
        if encrypted != 0 {
            self.push_clear()?;
            self.subsamples.push(Subsample {
                clear: 0,
                encrypted: u32::try_from(encrypted).map_err(|_| Error::TooLarge)?,
            });
        }
        Ok(())
    }

    fn push_clear(&mut self) -> Result<(), Error> {
        while self.clear > usize::from(u16::MAX) {
            self.subsamples.push(Subsample {
                clear: u16::MAX,
                encrypted: 0,
            });
            self.clear -= usize::from(u16::MAX);
        }
        if self.clear != 0 {
            self.subsamples.push(Subsample {
                clear: u16::try_from(self.clear).map_err(|_| Error::TooLarge)?,
                encrypted: 0,
            });
            self.clear = 0;
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<Vec<Subsample>, Error> {
        self.push_clear()?;
        let mut merged = Vec::<Subsample>::new();
        for subsample in self.subsamples {
            if subsample.encrypted != 0
                && let Some(previous) = merged.last_mut()
                && previous.encrypted == 0
            {
                previous.encrypted = subsample.encrypted;
                continue;
            }
            merged.push(subsample);
        }
        Ok(merged)
    }
}
