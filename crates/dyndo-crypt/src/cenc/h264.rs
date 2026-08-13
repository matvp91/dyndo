use ctr::cipher::StreamCipher;
use h264_reader::Context;
use h264_reader::nal::slice::SliceHeader;
use h264_reader::nal::{Nal, RefNal};
use h264_reader::rbsp::{BitRead, BitReaderError, Numeric, Primitive};

use super::{Aes128Ctr, Error, Subsample, SubsampleOrganizer};

pub(super) fn encrypt_sample(
    sample: &mut [u8],
    nal_length_size: u8,
    sequence_parameter_sets: &[Vec<u8>],
    picture_parameter_sets: &[Vec<u8>],
    cipher: &mut Aes128Ctr,
) -> Result<Vec<Subsample>, Error> {
    let length_size = usize::from(nal_length_size);
    if !(1..=4).contains(&length_size) {
        return Err(Error::InvalidMedia);
    }

    let context = context(sequence_parameter_sets, picture_parameter_sets)?;
    let mut organizer = SubsampleOrganizer::default();
    let mut offset = 0_usize;
    while offset < sample.len() {
        let length_end = offset.checked_add(length_size).ok_or(Error::TooLarge)?;
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
            length_size
                .checked_add(1 + slice_header_size(&context, nal)?)
                .ok_or(Error::TooLarge)?
        } else {
            length_size.checked_add(nal_size).ok_or(Error::TooLarge)?
        };
        let encrypted = length_size
            .checked_add(nal_size)
            .and_then(|total| total.checked_sub(clear))
            .ok_or(Error::InvalidMedia)?;
        organizer.add(clear, encrypted)?;
        offset = nal_end;
    }

    let subsamples = organizer.finish()?;
    let mut offset = 0_usize;
    for subsample in &subsamples {
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
    Ok(subsamples)
}

fn context(sps: &[Vec<u8>], pps: &[Vec<u8>]) -> Result<Context, Error> {
    let mut context = Context::new();
    for bytes in sps {
        let nal = RefNal::new(bytes, &[], true);
        let parsed = h264_reader::nal::sps::SeqParameterSet::from_bits(nal.rbsp_bits())
            .map_err(|_| Error::InvalidMedia)?;
        context.put_seq_param_set(parsed);
    }
    for bytes in pps {
        let nal = RefNal::new(bytes, &[], true);
        let parsed = h264_reader::nal::pps::PicParameterSet::from_bits(&context, nal.rbsp_bits())
            .map_err(|_| Error::InvalidMedia)?;
        context.put_pic_param_set(parsed);
    }
    Ok(context)
}

fn slice_header_size(context: &Context, bytes: &[u8]) -> Result<usize, Error> {
    let nal = RefNal::new(bytes, &[], true);
    let header = nal.header().map_err(|_| Error::InvalidMedia)?;
    let mut bits = CountingBits::new(nal.rbsp_bits());
    SliceHeader::from_bits(context, &mut bits, header).map_err(|_| Error::InvalidMedia)?;
    encoded_size(
        bytes.get(1..).ok_or(Error::InvalidMedia)?,
        bits.count.div_ceil(8) as usize,
    )
}

fn encoded_size(bytes: &[u8], rbsp_size: usize) -> Result<usize, Error> {
    let mut decoded = 0;
    let mut zeroes = 0;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if zeroes >= 2 && byte == 3 {
            zeroes = 0;
            continue;
        }
        decoded += 1;
        if decoded == rbsp_size {
            return Ok(index + 1);
        }
        zeroes = if byte == 0 { zeroes + 1 } else { 0 };
    }
    Err(Error::InvalidMedia)
}

struct CountingBits<R> {
    inner: R,
    count: u64,
}

impl<R> CountingBits<R> {
    fn new(inner: R) -> Self {
        Self { inner, count: 0 }
    }
}

impl<R: BitRead> BitRead for CountingBits<R> {
    fn read_ue(&mut self, name: &'static str) -> Result<u32, BitReaderError> {
        let mut leading_zeroes = 0;
        while !self.read_bool(name)? {
            leading_zeroes += 1;
            if leading_zeroes > 31 {
                return Err(BitReaderError::ExpGolombTooLarge(name));
            }
        }
        let suffix = if leading_zeroes == 0 {
            0
        } else {
            self.read::<u32>(leading_zeroes, name)?
        };
        Ok((1_u32 << leading_zeroes) - 1 + suffix)
    }

    fn read_se(&mut self, name: &'static str) -> Result<i32, BitReaderError> {
        let value = self.read_ue(name)?;
        Ok(if value & 1 == 0 {
            -(value as i32 / 2)
        } else {
            value.div_ceil(2) as i32
        })
    }

    fn read_bool(&mut self, name: &'static str) -> Result<bool, BitReaderError> {
        self.count += 1;
        self.inner.read_bool(name)
    }

    fn read<U: Numeric>(&mut self, bits: u32, name: &'static str) -> Result<U, BitReaderError> {
        self.count += u64::from(bits);
        self.inner.read(bits, name)
    }

    fn read_to<V: Primitive>(&mut self, name: &'static str) -> Result<V, BitReaderError> {
        self.count += (V::buffer().as_ref().len() * 8) as u64;
        self.inner.read_to(name)
    }

    fn skip(&mut self, bits: u32, name: &'static str) -> Result<(), BitReaderError> {
        self.count += u64::from(bits);
        self.inner.skip(bits, name)
    }

    fn has_more_rbsp_data(&mut self, name: &'static str) -> Result<bool, BitReaderError> {
        self.inner.has_more_rbsp_data(name)
    }

    fn finish_rbsp(self) -> Result<(), BitReaderError> {
        self.inner.finish_rbsp()
    }

    fn finish_sei_payload(self) -> Result<(), BitReaderError> {
        self.inner.finish_sei_payload()
    }
}
