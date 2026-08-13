use mp4_atom::{Saio, Saiz, Senc, SencBoxVersion, Tfhd, Trun};

use super::Error;
use super::mp4::{
    Mp4Box, add_delta, add_signed, decode_atom, encode_atom, find_box, find_child, grow_box,
};
use super::sample::{SampleEncryptionInfo, SampleEncryptor};
use crate::codec::CodecConfig;
use crate::drm::EncryptionConfig;

pub(super) struct FragmentEncryptor {
    samples: SampleEncryptor,
}

impl FragmentEncryptor {
    pub(super) fn new(config: &EncryptionConfig, codec: &CodecConfig) -> Result<Self, Error> {
        Ok(Self {
            samples: SampleEncryptor::new(config.key, codec)?,
        })
    }

    pub(super) fn encrypt(mut self, media: &[u8]) -> Result<Vec<u8>, Error> {
        let mut output = media.to_vec();
        let mut offset = 0_usize;
        while offset < output.len() {
            let moof = find_box(&output, offset, output.len(), b"moof")
                .map_err(|_| Error::InvalidMedia)?;
            let added = self.encrypt_fragment(&mut output, moof)?;
            let mdat = find_box(&output, moof.end + added, output.len(), b"mdat")
                .map_err(|_| Error::InvalidMedia)?;
            offset = mdat.end;
        }
        Ok(output)
    }

    fn encrypt_fragment(&mut self, bytes: &mut Vec<u8>, moof: Mp4Box) -> Result<usize, Error> {
        let traf = find_child(bytes, moof, b"traf", 0).map_err(|_| Error::InvalidMedia)?;
        let tfhd = find_child(bytes, traf, b"tfhd", 0).map_err(|_| Error::InvalidMedia)?;
        let trun = find_child(bytes, traf, b"trun", 0).map_err(|_| Error::InvalidMedia)?;
        let tfhd = decode_atom::<Tfhd>(bytes, tfhd)?;
        let mut trun_atom = decode_atom::<Trun>(bytes, trun)?;
        let sample_sizes = sample_sizes(&tfhd, &trun_atom)?;
        let data_offset = trun_atom.data_offset.ok_or(Error::InvalidMedia)?;
        let data_start = add_signed(moof.start, data_offset)?;
        let sample_info = self.encrypt_samples(bytes, data_start, &sample_sizes)?;
        let auxiliary = auxiliary_boxes(&sample_info, self.samples.uses_subsamples())?;

        let original_trun_size = trun.end - trun.start;
        let auxiliary_size = auxiliary.saiz.len() + auxiliary.saio.len() + auxiliary.senc.len();
        trun_atom.data_offset = Some(
            data_offset
                .checked_add(i32::try_from(auxiliary_size).map_err(|_| Error::TooLarge)?)
                .ok_or(Error::TooLarge)?,
        );
        let encoded_trun = encode_atom(&trun_atom)?;
        let trun_growth = encoded_trun.len() as isize - original_trun_size as isize;
        trun_atom.data_offset = Some(
            trun_atom
                .data_offset
                .and_then(|offset| offset.checked_add(i32::try_from(trun_growth).ok()?))
                .ok_or(Error::TooLarge)?,
        );
        let encoded_trun = encode_atom(&trun_atom)?;

        let senc_offset = add_delta(traf.end, trun_growth)?
            .checked_add(auxiliary.saiz.len() + auxiliary.saio.len() + 16)
            .and_then(|position| position.checked_sub(moof.start))
            .ok_or(Error::TooLarge)?;
        let saio = encode_atom(&Saio {
            offsets: vec![u64::try_from(senc_offset).map_err(|_| Error::TooLarge)?],
            ..Default::default()
        })?;
        let total_growth = trun_growth
            .checked_add(isize::try_from(auxiliary_size).map_err(|_| Error::TooLarge)?)
            .ok_or(Error::TooLarge)?;
        let added = usize::try_from(total_growth).map_err(|_| Error::InvalidMedia)?;

        bytes.splice(trun.start..trun.end, encoded_trun);
        let insertion = add_delta(traf.end, trun_growth)?;
        let mut auxiliary_data = auxiliary.saiz;
        auxiliary_data.extend_from_slice(&saio);
        auxiliary_data.extend_from_slice(&auxiliary.senc);
        bytes.splice(insertion..insertion, auxiliary_data);
        grow_box(bytes, traf.start, added)?;
        grow_box(bytes, moof.start, added)?;
        Ok(added)
    }

    fn encrypt_samples(
        &mut self,
        bytes: &mut [u8],
        mut sample_start: usize,
        sample_sizes: &[usize],
    ) -> Result<Vec<SampleEncryptionInfo>, Error> {
        let mut sample_info = Vec::with_capacity(sample_sizes.len());
        let mut first_iv = [0; 8];
        getrandom::getrandom(&mut first_iv).map_err(|_| Error::Random)?;
        for (index, size) in sample_sizes.iter().copied().enumerate() {
            let sample_end = sample_start
                .checked_add(size)
                .filter(|end| *end <= bytes.len())
                .ok_or(Error::InvalidMedia)?;
            let iv = u64::from_be_bytes(first_iv)
                .checked_add(u64::try_from(index).map_err(|_| Error::TooLarge)?)
                .ok_or(Error::TooLarge)?
                .to_be_bytes();
            sample_info.push(
                self.samples
                    .encrypt(&mut bytes[sample_start..sample_end], iv)?,
            );
            sample_start = sample_end;
        }
        Ok(sample_info)
    }
}

struct AuxiliaryBoxes {
    saiz: Vec<u8>,
    saio: Vec<u8>,
    senc: Vec<u8>,
}

fn auxiliary_boxes(
    sample_info: &[SampleEncryptionInfo],
    use_subsamples: bool,
) -> Result<AuxiliaryBoxes, Error> {
    let sample_info_sizes = sample_info
        .iter()
        .map(|info| sample_info_size(info, use_subsamples))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AuxiliaryBoxes {
        saiz: encode_atom(&Saiz {
            default_sample_info_size: if use_subsamples { 0 } else { 8 },
            sample_count: u32::try_from(sample_info.len()).map_err(|_| Error::TooLarge)?,
            sample_info_size: if use_subsamples {
                sample_info_sizes
            } else {
                Vec::new()
            },
            ..Default::default()
        })?,
        saio: encode_atom(&Saio {
            offsets: vec![0],
            ..Default::default()
        })?,
        senc: encode_atom(&Senc {
            version: SencBoxVersion::V0,
            use_subsamples,
            data: senc_data(sample_info, use_subsamples)?,
        })?,
    })
}

fn sample_sizes(tfhd: &Tfhd, trun: &Trun) -> Result<Vec<usize>, Error> {
    trun.entries
        .iter()
        .map(|entry| {
            entry
                .size
                .or(tfhd.default_sample_size)
                .map(|size| size as usize)
                .ok_or(Error::InvalidMedia)
        })
        .collect()
}

fn sample_info_size(info: &SampleEncryptionInfo, use_subsamples: bool) -> Result<u8, Error> {
    let size = 8_usize
        .checked_add(if use_subsamples { 2 } else { 0 })
        .and_then(|size| size.checked_add(info.subsamples.len() * 6))
        .ok_or(Error::TooLarge)?;
    u8::try_from(size).map_err(|_| Error::TooLarge)
}

fn senc_data(info: &[SampleEncryptionInfo], use_subsamples: bool) -> Result<Vec<u8>, Error> {
    let mut data = Vec::new();
    data.extend_from_slice(
        &u32::try_from(info.len())
            .map_err(|_| Error::TooLarge)?
            .to_be_bytes(),
    );
    for sample in info {
        data.extend_from_slice(&sample.iv);
        if use_subsamples {
            data.extend_from_slice(
                &u16::try_from(sample.subsamples.len())
                    .map_err(|_| Error::TooLarge)?
                    .to_be_bytes(),
            );
            for subsample in &sample.subsamples {
                data.extend_from_slice(&subsample.clear.to_be_bytes());
                data.extend_from_slice(&subsample.encrypted.to_be_bytes());
            }
        }
    }
    Ok(data)
}
