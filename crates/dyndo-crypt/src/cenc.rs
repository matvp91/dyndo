use crate::encryption_config::{EncryptionConfig, EncryptionScheme};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid or unsupported CMAF initialization segment")]
    InvalidInit,
    #[error("unsupported encryption scheme")]
    UnsupportedScheme,
    #[error("initialization segment is too large")]
    TooLarge,
}

#[derive(Clone, Copy)]
struct Mp4Box {
    start: usize,
    end: usize,
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
