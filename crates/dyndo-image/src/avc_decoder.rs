//! Decoding the keyframe a fragment opens on.
//!
//! Storage and decoders disagree on how a frame's NAL units are delimited: a CMAF
//! sample prefixes each unit with its length, while a decoder wants them separated
//! by Annex-B start codes and preceded by the parameter sets that describe them.
//! That translation, and the decoder it feeds, live here.

use std::io::Cursor;

use mp4_atom::{Atom, Codec, Header, Mdat, Moov, ReadAtom, ReadFrom};
use openh264::formats::YUVSource;

use crate::ThumbnailError;

/// Separates NAL units in a decoder's input, where a length prefix separates them
/// in storage.
const START_CODE: [u8; 4] = [0, 0, 0, 1];

const NAL_NON_IDR: u8 = 1;
const NAL_IDR: u8 = 5;
const NAL_SPS: u8 = 7;
const NAL_PPS: u8 = 8;

/// One decoded picture, as packed 8-bit RGB.
pub(crate) struct Frame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgb: Vec<u8>,
}

/// A decoder configured by one track's initialization segment: openh264, plus the
/// parameter sets to prefix a keyframe with and the width of the length field the
/// stored NAL units sit behind.
pub(crate) struct AvcDecoder {
    inner: openh264::decoder::Decoder,
    parameters: Vec<u8>,
    length_size: usize,
}

impl AvcDecoder {
    pub(crate) fn new(initialization: &[u8]) -> Result<Self, ThumbnailError> {
        let moov = read_moov(initialization)?;
        let sample_entry = moov
            .trak
            .first()
            .and_then(|track| track.mdia.minf.stbl.stsd.codecs.first())
            .ok_or(ThumbnailError::Container("stsd has no sample entry"))?;
        let Codec::Avc1(entry) = sample_entry else {
            return Err(ThumbnailError::Container("sample entry is not avc1"));
        };

        let mut parameters = Vec::new();
        for set in entry
            .avcc
            .sequence_parameter_sets
            .iter()
            .chain(&entry.avcc.picture_parameter_sets)
        {
            parameters.extend_from_slice(&START_CODE);
            parameters.extend_from_slice(set);
        }

        Ok(Self {
            inner: openh264::decoder::Decoder::new()?,
            parameters,
            length_size: usize::from(entry.avcc.length_size),
        })
    }

    /// Decodes the keyframe `fragment` opens on.
    ///
    /// Every fragment opens on an IDR, so a frame never depends on the one decoded
    /// before it: no picture here means a broken source, not a warm-up.
    pub(crate) fn frame(&mut self, fragment: &[u8]) -> Result<Frame, ThumbnailError> {
        let packet = self.keyframe(fragment)?;
        let picture = self
            .inner
            .decode(&packet)?
            .ok_or(ThumbnailError::EmptyFrame)?;
        let (width, height) = picture.dimensions();
        let mut rgb = vec![0u8; width * height * 3];
        picture.write_rgb8(&mut rgb);

        Ok(Frame {
            width: width as u32,
            height: height as u32,
            rgb,
        })
    }

    /// The Annex-B packet holding the keyframe `fragment` opens on, prefixed with
    /// the parameter sets that make it decodable with nothing else.
    ///
    /// Parameter sets the source repeats in-band are appended after those, since a
    /// stream may change them at an IDR and the later ones win.
    fn keyframe(&self, fragment: &[u8]) -> Result<Vec<u8>, ThumbnailError> {
        let media = read_mdat(fragment)?;
        let mut packet = self.parameters.clone();
        let mut offset = 0;

        while offset + self.length_size <= media.len() {
            let length = media[offset..offset + self.length_size]
                .iter()
                .fold(0usize, |length, byte| (length << 8) | usize::from(*byte));
            offset += self.length_size;
            let unit = media
                .get(offset..offset + length)
                .ok_or(ThumbnailError::Container("nal unit runs past the mdat"))?;
            offset += length;

            match unit.first().map(|byte| byte & 0x1f) {
                Some(kind @ (NAL_SPS | NAL_PPS | NAL_IDR)) => {
                    packet.extend_from_slice(&START_CODE);
                    packet.extend_from_slice(unit);
                    if kind == NAL_IDR {
                        return Ok(packet);
                    }
                }
                // A coded slice that is not an IDR, reached before any IDR, means
                // the fragment does not open on a keyframe after all.
                Some(NAL_NON_IDR) => return Err(ThumbnailError::NoKeyframe),
                Some(_) => {}
                None => return Err(ThumbnailError::Container("empty nal unit")),
            }
        }

        Err(ThumbnailError::NoKeyframe)
    }
}

/// Walks the top-level boxes of an initialization segment to its `moov`.
fn read_moov(initialization: &[u8]) -> Result<Moov, ThumbnailError> {
    let mut cursor = Cursor::new(initialization);

    loop {
        let header = Header::read_from(&mut cursor)?;
        let size = header
            .size
            .ok_or(ThumbnailError::Container("box has no size"))?;
        if header.kind == Moov::KIND {
            return Ok(Moov::read_atom(&header, &mut cursor)?);
        }
        cursor.set_position(cursor.position() + size as u64);
    }
}

/// The payload of a fragment's `mdat`, which is its samples one after another.
fn read_mdat(fragment: &[u8]) -> Result<&[u8], ThumbnailError> {
    let mut cursor = Cursor::new(fragment);

    loop {
        let header = Header::read_from(&mut cursor)?;
        let size = header
            .size
            .ok_or(ThumbnailError::Container("box has no size"))?;
        let start = cursor.position() as usize;
        if header.kind == Mdat::KIND {
            return fragment
                .get(start..start + size)
                .ok_or(ThumbnailError::Container("mdat runs past the fragment"));
        }
        cursor.set_position((start + size) as u64);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

    #[test]
    fn new_takes_the_parameter_sets_from_an_avc_initialization_segment() {
        let decoder = decoder();

        assert!(decoder.parameters.starts_with(&START_CODE) && decoder.length_size == 4);
    }

    /// Both parameter sets are carried, so the second start code opens the picture
    /// parameter set rather than the prefix ending after the sequence one.
    #[test]
    fn new_carries_both_parameter_sets() {
        let decoder = decoder();

        assert_eq!(
            decoder
                .parameters
                .windows(START_CODE.len())
                .filter(|window| *window == START_CODE)
                .count(),
            2
        );
    }

    #[test]
    fn new_refuses_a_track_that_is_not_avc() {
        let Err(error) = AvcDecoder::new(&fixture("video_av1_240.mp4")) else {
            panic!("a track that is not avc unexpectedly configured a decoder");
        };

        assert!(matches!(error, ThumbnailError::Container(_)), "{error}");
    }

    #[test]
    fn keyframe_prefixes_the_parameter_sets_to_the_idr() {
        let decoder = decoder();

        let packet = decoder.keyframe(&fragment(&[(NAL_IDR, 3)])).unwrap();

        assert_eq!(
            packet,
            [&decoder.parameters[..], &START_CODE, &[NAL_IDR, 0, 0]].concat()
        );
    }

    #[test]
    fn keyframe_skips_units_before_the_idr_it_does_not_need() {
        let decoder = decoder();

        let packet = decoder
            .keyframe(&fragment(&[(6, 2), (NAL_IDR, 3)]))
            .unwrap();

        assert_eq!(
            packet.len(),
            decoder.parameters.len() + START_CODE.len() + 3
        );
    }

    #[test]
    fn keyframe_carries_parameter_sets_the_source_repeats_in_band() {
        let decoder = decoder();

        let packet = decoder
            .keyframe(&fragment(&[(NAL_SPS, 2), (NAL_PPS, 2), (NAL_IDR, 3)]))
            .unwrap();

        assert_eq!(
            packet.len(),
            decoder.parameters.len() + START_CODE.len() * 3 + 2 + 2 + 3
        );
    }

    #[test]
    fn keyframe_stops_at_the_first_idr() {
        let decoder = decoder();

        let packet = decoder
            .keyframe(&fragment(&[(NAL_IDR, 3), (NAL_IDR, 9)]))
            .unwrap();

        assert_eq!(
            packet.len(),
            decoder.parameters.len() + START_CODE.len() + 3
        );
    }

    #[test]
    fn keyframe_refuses_a_fragment_that_opens_on_a_coded_slice() {
        let error = decoder()
            .keyframe(&fragment(&[(NAL_NON_IDR, 3)]))
            .unwrap_err();

        assert!(matches!(error, ThumbnailError::NoKeyframe), "{error}");
    }

    #[test]
    fn keyframe_refuses_a_fragment_holding_no_coded_slice() {
        let error = decoder().keyframe(&fragment(&[])).unwrap_err();

        assert!(matches!(error, ThumbnailError::NoKeyframe), "{error}");
    }

    #[test]
    fn keyframe_refuses_a_unit_whose_length_runs_past_the_mdat() {
        let media = [&u32::MAX.to_be_bytes()[..], &[NAL_IDR, 0, 0]].concat();
        let fragment = [box_bytes(b"moof", &[]), box_bytes(b"mdat", &media)].concat();

        let error = decoder().keyframe(&fragment).unwrap_err();

        assert!(matches!(error, ThumbnailError::Container(_)), "{error}");
    }

    #[test]
    fn read_mdat_refuses_a_fragment_without_one() {
        let error = read_mdat(&box_bytes(b"moof", &[])).unwrap_err();

        assert!(matches!(error, ThumbnailError::Parse(_)), "{error}");
    }

    fn decoder() -> AvcDecoder {
        AvcDecoder::new(&fixture("video_avc_1080.mp4")).unwrap()
    }

    fn fixture(name: &str) -> Vec<u8> {
        fs::read(format!("{FIXTURES}/{name}")).unwrap()
    }

    /// A `moof`/`mdat` pair whose samples hold one length-prefixed NAL unit per
    /// `(kind, length)` given. Only the length prefix and the first byte of each
    /// unit carry meaning here, so the rest is left zeroed.
    fn fragment(units: &[(u8, usize)]) -> Vec<u8> {
        let mut media = Vec::new();
        for (kind, length) in units {
            media.extend_from_slice(&(*length as u32).to_be_bytes());
            media.push(*kind);
            media.extend(std::iter::repeat_n(0, length - 1));
        }

        [box_bytes(b"moof", &[]), box_bytes(b"mdat", &media)].concat()
    }

    fn box_bytes(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(body);
        bytes
    }
}
