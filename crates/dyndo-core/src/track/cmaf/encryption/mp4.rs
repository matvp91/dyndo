use mp4_atom::{Decode, Encode};

use super::Error;

#[derive(Clone, Copy)]
pub(super) struct Mp4Box {
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(super) fn decode_atom<T: Decode>(bytes: &[u8], atom: Mp4Box) -> Result<T, Error> {
    let mut encoded = bytes.get(atom.start..atom.end).ok_or(Error::InvalidMedia)?;
    Ok(T::decode(&mut encoded)?)
}

pub(super) fn encode_atom(atom: &impl Encode) -> Result<Vec<u8>, Error> {
    let mut encoded = Vec::new();
    atom.encode(&mut encoded)?;
    Ok(encoded)
}

pub(super) fn add_delta(position: usize, delta: isize) -> Result<usize, Error> {
    position.checked_add_signed(delta).ok_or(Error::TooLarge)
}

pub(super) fn add_signed(base: usize, offset: i32) -> Result<usize, Error> {
    if offset < 0 {
        base.checked_sub(offset.unsigned_abs() as usize)
    } else {
        base.checked_add(offset as usize)
    }
    .ok_or(Error::InvalidMedia)
}

pub(super) fn find_child(
    bytes: &[u8],
    parent: Mp4Box,
    kind: &[u8; 4],
    skip: usize,
) -> Result<Mp4Box, Error> {
    let start = parent
        .start
        .checked_add(8 + skip)
        .ok_or(Error::InvalidInit)?;
    find_box(bytes, start, parent.end, kind)
}

pub(super) fn find_box(
    bytes: &[u8],
    mut offset: usize,
    end: usize,
    kind: &[u8; 4],
) -> Result<Mp4Box, Error> {
    while offset < end {
        let atom = read_box(bytes, offset, end)?;
        if bytes.get(offset + 4..offset + 8) == Some(kind) {
            return Ok(atom);
        }
        offset = atom.end;
    }
    Err(Error::InvalidInit)
}

pub(super) fn read_box(bytes: &[u8], start: usize, limit: usize) -> Result<Mp4Box, Error> {
    let header = bytes.get(start..start + 8).ok_or(Error::InvalidInit)?;
    let size = u32::from_be_bytes(header[..4].try_into().map_err(|_| Error::InvalidInit)?) as usize;
    let end = start.checked_add(size).ok_or(Error::InvalidInit)?;
    if size < 8 || end > limit {
        return Err(Error::InvalidInit);
    }
    Ok(Mp4Box { start, end })
}

pub(super) fn grow_box(bytes: &mut [u8], start: usize, amount: usize) -> Result<(), Error> {
    let size = bytes.get(start..start + 4).ok_or(Error::InvalidInit)?;
    let size = u32::from_be_bytes(size.try_into().map_err(|_| Error::InvalidInit)?);
    let amount = u32::try_from(amount).map_err(|_| Error::TooLarge)?;
    let size = size.checked_add(amount).ok_or(Error::TooLarge)?;
    bytes[start..start + 4].copy_from_slice(&size.to_be_bytes());
    Ok(())
}
