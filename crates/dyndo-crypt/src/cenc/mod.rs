use aes::Aes128;
use ctr::cipher::{KeyIvInit, StreamCipher};
use mp4_atom::{Decode, Encode, Saio, Saiz, Senc, SencBoxVersion, Tfhd, Trun};

use crate::encryption_config::{EncryptionConfig, EncryptionScheme};

mod h264;

type Aes128Ctr = ctr::Ctr128BE<Aes128>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid or unsupported CMAF initialization segment")]
    InvalidInit,
    #[error("unsupported encryption scheme")]
    UnsupportedScheme,
    #[error("initialization segment is too large")]
    TooLarge,
    #[error("invalid or unsupported CMAF media segment")]
    InvalidMedia,
    #[error("could not generate a sample IV")]
    Random,
    #[error("invalid MP4 box")]
    Atom(#[from] mp4_atom::Error),
}

#[derive(Clone, Copy)]
struct Mp4Box {
    start: usize,
    end: usize,
}

#[derive(Clone)]
pub enum SampleEncryption {
    FullSample,
    Avc {
        nal_length_size: u8,
        sequence_parameter_sets: Vec<Vec<u8>>,
        picture_parameter_sets: Vec<Vec<u8>>,
    },
}

struct SampleEncryptionInfo {
    iv: [u8; 8],
    subsamples: Vec<Subsample>,
}

struct Subsample {
    clear: u16,
    encrypted: u32,
}

pub fn encrypt_init(init: &[u8], config: &EncryptionConfig) -> Result<Vec<u8>, Error> {
    if config.scheme != EncryptionScheme::Cenc {
        return Err(Error::UnsupportedScheme);
    }

    let moov = find_box(init, 0, init.len(), b"moov")?;
    let trak = find_child(init, moov, b"trak", 0)?;
    let mdia = find_child(init, trak, b"mdia", 0)?;
    let minf = find_child(init, mdia, b"minf", 0)?;
    let stbl = find_child(init, minf, b"stbl", 0)?;
    let stsd = find_child(init, stbl, b"stsd", 0)?;
    let entry = first_sample_entry(init, stsd)?;
    let format: [u8; 4] = init[entry.start + 4..entry.start + 8]
        .try_into()
        .map_err(|_| Error::InvalidInit)?;
    let encrypted_format = match &format {
        b"avc1" | b"avc3" | b"hvc1" | b"hev1" | b"av01" => *b"encv",
        b"mp4a" | b"ac-3" | b"ec-3" => *b"enca",
        _ => return Err(Error::InvalidInit),
    };

    let sinf = sinf(format, config);
    let pssh: Vec<u8> = config
        .drm_systems
        .iter()
        .flat_map(|system| system.pssh.iter().copied())
        .collect();
    let mut output = Vec::with_capacity(init.len() + sinf.len() + pssh.len());
    output.extend_from_slice(&init[..entry.end]);
    output.extend_from_slice(&sinf);
    output.extend_from_slice(&init[entry.end..moov.end]);
    output.extend_from_slice(&pssh);
    output.extend_from_slice(&init[moov.end..]);
    output[entry.start + 4..entry.start + 8].copy_from_slice(&encrypted_format);

    for parent in [entry, stsd, stbl, minf, mdia, trak] {
        grow_box(&mut output, parent.start, sinf.len())?;
    }
    grow_box(&mut output, moov.start, sinf.len() + pssh.len())?;

    Ok(output)
}

pub fn encrypt_media(
    media: &[u8],
    config: &EncryptionConfig,
    sample_encryption: SampleEncryption,
) -> Result<Vec<u8>, Error> {
    if config.scheme != EncryptionScheme::Cenc {
        return Err(Error::UnsupportedScheme);
    }

    let mut output = media.to_vec();
    let mut offset = 0_usize;
    while offset < output.len() {
        let moof =
            find_box(&output, offset, output.len(), b"moof").map_err(|_| Error::InvalidMedia)?;
        let added = encrypt_fragment(&mut output, moof, config, &sample_encryption)?;
        let mdat = find_box(&output, moof.end + added, output.len(), b"mdat")
            .map_err(|_| Error::InvalidMedia)?;
        offset = mdat.end;
    }
    Ok(output)
}

fn encrypt_fragment(
    bytes: &mut Vec<u8>,
    moof: Mp4Box,
    config: &EncryptionConfig,
    sample_encryption: &SampleEncryption,
) -> Result<usize, Error> {
    let traf = find_child(bytes, moof, b"traf", 0).map_err(|_| Error::InvalidMedia)?;
    let tfhd = find_child(bytes, traf, b"tfhd", 0).map_err(|_| Error::InvalidMedia)?;
    let trun = find_child(bytes, traf, b"trun", 0).map_err(|_| Error::InvalidMedia)?;
    let tfhd = decode_atom::<Tfhd>(bytes, tfhd)?;
    let mut trun_atom = decode_atom::<Trun>(bytes, trun)?;
    let sample_sizes = sample_sizes(&tfhd, &trun_atom)?;
    let data_offset = trun_atom.data_offset.ok_or(Error::InvalidMedia)?;
    let data_start = add_signed(moof.start, data_offset)?;

    let mut sample_start = data_start;
    let mut sample_info = Vec::with_capacity(sample_sizes.len());
    let mut iv = [0; 8];
    getrandom::getrandom(&mut iv).map_err(|_| Error::Random)?;
    for (index, size) in sample_sizes.iter().copied().enumerate() {
        let sample_end = sample_start
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or(Error::InvalidMedia)?;
        let sample_iv = u64::from_be_bytes(iv)
            .checked_add(u64::try_from(index).map_err(|_| Error::TooLarge)?)
            .ok_or(Error::TooLarge)?
            .to_be_bytes();
        let subsamples = encrypt_sample(
            &mut bytes[sample_start..sample_end],
            &config.key,
            sample_iv,
            sample_encryption,
        )?;
        sample_info.push(SampleEncryptionInfo {
            iv: sample_iv,
            subsamples,
        });
        sample_start = sample_end;
    }

    let use_subsamples = matches!(sample_encryption, SampleEncryption::Avc { .. });
    let sample_info_sizes = sample_info
        .iter()
        .map(|info| sample_info_size(info, use_subsamples))
        .collect::<Result<Vec<_>, _>>()?;
    let saiz = encode_atom(&Saiz {
        default_sample_info_size: if use_subsamples { 0 } else { 8 },
        sample_count: u32::try_from(sample_sizes.len()).map_err(|_| Error::TooLarge)?,
        sample_info_size: if use_subsamples {
            sample_info_sizes
        } else {
            Vec::new()
        },
        ..Default::default()
    })?;
    let senc = encode_atom(&Senc {
        version: SencBoxVersion::V0,
        use_subsamples,
        data: senc_data(&sample_info, use_subsamples)?,
    })?;
    let placeholder_saio = encode_atom(&Saio {
        offsets: vec![0],
        ..Default::default()
    })?;

    let original_trun_size = trun.end - trun.start;
    let auxiliary_size = saiz.len() + placeholder_saio.len() + senc.len();
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
        .checked_add(saiz.len() + placeholder_saio.len() + 16)
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
    let mut auxiliary = saiz;
    auxiliary.extend_from_slice(&saio);
    auxiliary.extend_from_slice(&senc);
    bytes.splice(insertion..insertion, auxiliary);
    grow_box(bytes, traf.start, added)?;
    grow_box(bytes, moof.start, added)?;
    Ok(added)
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

fn decode_atom<T: Decode>(bytes: &[u8], atom: Mp4Box) -> Result<T, Error> {
    let mut encoded = bytes.get(atom.start..atom.end).ok_or(Error::InvalidMedia)?;
    Ok(T::decode(&mut encoded)?)
}

fn encode_atom(atom: &impl Encode) -> Result<Vec<u8>, Error> {
    let mut encoded = Vec::new();
    atom.encode(&mut encoded)?;
    Ok(encoded)
}

fn add_delta(position: usize, delta: isize) -> Result<usize, Error> {
    position.checked_add_signed(delta).ok_or(Error::TooLarge)
}

fn add_signed(base: usize, offset: i32) -> Result<usize, Error> {
    if offset < 0 {
        base.checked_sub(offset.unsigned_abs() as usize)
    } else {
        base.checked_add(offset as usize)
    }
    .ok_or(Error::InvalidMedia)
}

fn encrypt_sample(
    sample: &mut [u8],
    key: &[u8; 16],
    iv: [u8; 8],
    method: &SampleEncryption,
) -> Result<Vec<Subsample>, Error> {
    let mut counter = [0; 16];
    counter[..8].copy_from_slice(&iv);
    let mut cipher = Aes128Ctr::new(&(*key).into(), &counter.into());
    match method {
        SampleEncryption::FullSample => {
            cipher.apply_keystream(sample);
            Ok(Vec::new())
        }
        SampleEncryption::Avc {
            nal_length_size,
            sequence_parameter_sets,
            picture_parameter_sets,
        } => h264::encrypt_sample(
            sample,
            *nal_length_size,
            sequence_parameter_sets,
            picture_parameter_sets,
            &mut cipher,
        ),
    }
}

#[derive(Default)]
struct SubsampleOrganizer {
    subsamples: Vec<Subsample>,
    clear: usize,
}

impl SubsampleOrganizer {
    fn add(&mut self, mut clear: usize, mut encrypted: usize) -> Result<(), Error> {
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

    fn finish(mut self) -> Result<Vec<Subsample>, Error> {
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

fn sinf(format: [u8; 4], config: &EncryptionConfig) -> Vec<u8> {
    let frma = mp4_box(*b"frma", &format);

    let mut schm_body = Vec::from([0, 0, 0, 0]);
    schm_body.extend_from_slice(b"cenc");
    schm_body.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    let schm = mp4_box(*b"schm", &schm_body);

    let mut tenc_body = Vec::from([0, 0, 0, 0, 0, 0, 1, 8]);
    tenc_body.extend_from_slice(config.kid.as_bytes());
    let tenc = mp4_box(*b"tenc", &tenc_body);
    let schi = mp4_box(*b"schi", &tenc);

    let mut body = frma;
    body.extend_from_slice(&schm);
    body.extend_from_slice(&schi);
    mp4_box(*b"sinf", &body)
}

fn mp4_box(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(body.len() + 8);
    bytes.extend_from_slice(
        &u32::try_from(body.len() + 8)
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&kind);
    bytes.extend_from_slice(body);
    bytes
}

fn first_sample_entry(bytes: &[u8], stsd: Mp4Box) -> Result<Mp4Box, Error> {
    let start = stsd.start.checked_add(16).ok_or(Error::InvalidInit)?;
    read_box(bytes, start, stsd.end)
}

fn find_child(bytes: &[u8], parent: Mp4Box, kind: &[u8; 4], skip: usize) -> Result<Mp4Box, Error> {
    let start = parent
        .start
        .checked_add(8 + skip)
        .ok_or(Error::InvalidInit)?;
    find_box(bytes, start, parent.end, kind)
}

fn find_box(bytes: &[u8], mut offset: usize, end: usize, kind: &[u8; 4]) -> Result<Mp4Box, Error> {
    while offset < end {
        let atom = read_box(bytes, offset, end)?;
        if bytes.get(offset + 4..offset + 8) == Some(kind) {
            return Ok(atom);
        }
        offset = atom.end;
    }
    Err(Error::InvalidInit)
}

fn read_box(bytes: &[u8], start: usize, limit: usize) -> Result<Mp4Box, Error> {
    let header = bytes.get(start..start + 8).ok_or(Error::InvalidInit)?;
    let size = u32::from_be_bytes(header[..4].try_into().map_err(|_| Error::InvalidInit)?) as usize;
    let end = start.checked_add(size).ok_or(Error::InvalidInit)?;
    if size < 8 || end > limit {
        return Err(Error::InvalidInit);
    }
    Ok(Mp4Box { start, end })
}

fn grow_box(bytes: &mut [u8], start: usize, amount: usize) -> Result<(), Error> {
    let size = bytes.get(start..start + 4).ok_or(Error::InvalidInit)?;
    let size = u32::from_be_bytes(size.try_into().map_err(|_| Error::InvalidInit)?);
    let amount = u32::try_from(amount).map_err(|_| Error::TooLarge)?;
    let size = size.checked_add(amount).ok_or(Error::TooLarge)?;
    bytes[start..start + 4].copy_from_slice(&size.to_be_bytes());
    Ok(())
}
