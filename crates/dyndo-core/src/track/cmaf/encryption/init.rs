use super::Error;
use super::mp4::{Mp4Box, find_box, find_child, grow_box, read_box};
use crate::drm::EncryptionConfig;

pub(super) struct InitSegmentEncryptor<'a> {
    config: &'a EncryptionConfig,
}

impl<'a> InitSegmentEncryptor<'a> {
    pub(super) fn new(config: &'a EncryptionConfig) -> Self {
        Self { config }
    }

    pub(super) fn encrypt(&self, init: &[u8]) -> Result<Vec<u8>, Error> {
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
        let encrypted_format = protected_format(format)?;

        let sinf = sinf(format, self.config);
        let pssh: Vec<u8> = self
            .config
            .protection
            .systems
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
}

fn protected_format(format: [u8; 4]) -> Result<[u8; 4], Error> {
    match &format {
        b"avc1" | b"avc3" | b"hvc1" | b"hev1" | b"av01" => Ok(*b"encv"),
        b"mp4a" | b"ac-3" | b"ec-3" => Ok(*b"enca"),
        _ => Err(Error::InvalidInit),
    }
}

fn sinf(format: [u8; 4], config: &EncryptionConfig) -> Vec<u8> {
    let frma = mp4_box(*b"frma", &format);

    let mut schm_body = Vec::from([0, 0, 0, 0]);
    schm_body.extend_from_slice(b"cenc");
    schm_body.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    let schm = mp4_box(*b"schm", &schm_body);

    let mut tenc_body = Vec::from([0, 0, 0, 0, 0, 0, 1, 8]);
    tenc_body.extend_from_slice(config.protection.kid.as_bytes());
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
