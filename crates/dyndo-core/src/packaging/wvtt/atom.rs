use mp4_atom::{Atom, Buf, BufMut, Decode, Encode, FourCC};

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
