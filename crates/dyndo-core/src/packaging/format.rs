use mp4_atom::{BufMut, Codec, FourCC, Ftyp, Styp};

pub(crate) trait Format {
    type Payload;

    fn file_type(&self) -> Ftyp;

    fn segment_type(&self) -> Styp;

    fn handler(&self) -> FourCC;

    fn sample_entry(&self) -> Codec;

    fn write_sample<B: BufMut>(
        &self,
        payload: &Self::Payload,
        output: &mut B,
    ) -> mp4_atom::Result<()>;

    fn read_sample(&self, bytes: &[u8]) -> mp4_atom::Result<Self::Payload>;
}
