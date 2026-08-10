use mp4_atom::{Any, DecodeMaybe, Moof};

use super::format::Format;
use super::{UnpackageError, UnpackagedMedia, media_segment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Unpackager<F> {
    format: F,
}

impl<F> Unpackager<F> {
    pub(crate) fn new(format: F) -> Self {
        Self { format }
    }
}

impl<F: Format> Unpackager<F> {
    pub(crate) fn unpackage(
        &self,
        bytes: &[u8],
    ) -> Result<UnpackagedMedia<F::Payload>, UnpackageError> {
        let mut timescale = None;
        let mut segments = Vec::new();
        let mut header: Option<Moof> = None;
        let mut buf = bytes;

        while let Some(atom) = Any::decode_maybe(&mut buf)? {
            match atom {
                Any::Moov(moov) => {
                    timescale = moov
                        .trak
                        .first()
                        .map(|track| track.mdia.mdhd.timescale)
                        .filter(|timescale| *timescale != 0);
                }
                Any::Moof(moof) => {
                    if header.replace(moof).is_some() {
                        return Err(UnpackageError::UnpairedMediaSegment);
                    }
                }
                Any::Mdat(mdat) => {
                    let header = header.take().ok_or(UnpackageError::UnpairedMediaSegment)?;
                    segments.push(media_segment::read(&self.format, &header, &mdat.data)?);
                }
                _ => {}
            }
        }

        if header.is_some() {
            return Err(UnpackageError::UnpairedMediaSegment);
        }

        let timescale = timescale.ok_or(UnpackageError::MissingTimescale)?;
        Ok(UnpackagedMedia::new(timescale, segments))
    }
}
