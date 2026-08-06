//! The cue boxes [`mp4_atom`] does not model.
//!
//! ISO/IEC 14496-30 fills a text sample with cue boxes: a `vttc` for each cue on
//! screen over it, or a lone `vtte` where none is. Only a cue's payload is
//! modelled here — styling, positioning, and cue identifiers are optional boxes
//! dyndo neither reads nor writes.
//!
//! Not to be confused with [`mp4_atom::VttC`], which mp4-atom does carry: that is
//! the `vttC` configuration box in the sample entry, one capital and one nesting
//! level away from the `vttc` cue box here.

use mp4_atom::{Atom, Buf, BufMut, Decode, Encode, FourCC};

/// `VTTCueBox`: one cue on screen over a sample, leaving the payload as its only
/// child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Vttc {
    pub(super) payl: Payl,
}

impl Atom for Vttc {
    const KIND: FourCC = FourCC::new(b"vttc");

    fn decode_body<B: Buf>(buf: &mut B) -> mp4_atom::Result<Self> {
        Ok(Self {
            payl: Payl::decode(buf)?,
        })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> mp4_atom::Result<()> {
        self.payl.encode(buf)
    }
}

/// `CuePayloadBox`: the cue text, as UTF-8 filling the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Payl {
    pub(super) text: String,
}

impl Atom for Payl {
    const KIND: FourCC = FourCC::new(b"payl");

    fn decode_body<B: Buf>(buf: &mut B) -> mp4_atom::Result<Self> {
        let size = buf.remaining();
        let text = String::from_utf8(buf.slice(size).to_vec())
            .map_err(|error| mp4_atom::Error::InvalidString(error.to_string()))?;
        buf.advance(size);

        Ok(Self { text })
    }

    fn encode_body<B: BufMut>(&self, buf: &mut B) -> mp4_atom::Result<()> {
        self.text.as_bytes().encode(buf)
    }
}

/// `VTTEmptyCueBox`: a sample covering an interval with nothing on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Vtte;

impl Atom for Vtte {
    const KIND: FourCC = FourCC::new(b"vtte");

    fn decode_body<B: Buf>(_buf: &mut B) -> mp4_atom::Result<Self> {
        Ok(Self)
    }

    fn encode_body<B: BufMut>(&self, _buf: &mut B) -> mp4_atom::Result<()> {
        Ok(())
    }
}
