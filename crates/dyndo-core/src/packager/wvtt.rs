use mp4_atom::{
    Atom, Buf, BufMut, Codec, Decode, Encode, FourCC, Ftyp, PlainText, Styp, VttC, Wvtt,
};

use super::Format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WvttSample {
    cues: Vec<String>,
}

impl WvttSample {
    pub fn new(cues: Vec<String>) -> Self {
        Self { cues }
    }

    pub fn cues(&self) -> &[String] {
        &self.cues
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The CMAF format implementation for WebVTT-in-ISOBMFF (`wvtt`) samples.
pub struct WvttFormat;

impl Format for WvttFormat {
    type Payload = WvttSample;

    fn file_type(&self) -> Ftyp {
        Ftyp {
            major_brand: FourCC::new(b"iso6"),
            minor_version: 0,
            compatible_brands: vec![
                FourCC::new(b"iso6"),
                FourCC::new(b"cmfc"),
                FourCC::new(b"cmft"),
            ],
        }
    }

    fn segment_type(&self) -> Styp {
        Styp {
            major_brand: FourCC::new(b"msdh"),
            minor_version: 0,
            compatible_brands: vec![
                FourCC::new(b"msdh"),
                FourCC::new(b"msix"),
                FourCC::new(b"cmfs"),
            ],
        }
    }

    fn handler(&self) -> FourCC {
        FourCC::new(b"text")
    }

    fn sample_entry(&self) -> Codec {
        Codec::Wvtt(Wvtt {
            plaintext: PlainText {
                data_reference_index: 1,
            },
            config: VttC {
                config: "WEBVTT\n".to_string(),
            },
            label: None,
            btrt: None,
        })
    }

    fn write_sample<B: BufMut>(&self, sample: &WvttSample, output: &mut B) -> mp4_atom::Result<()> {
        if sample.cues().is_empty() {
            return Vtte.encode(output);
        }

        for cue in sample.cues() {
            Vttc {
                payl: Payl { text: cue.clone() },
            }
            .encode(output)?;
        }

        Ok(())
    }
}

struct Vttc {
    payl: Payl,
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

struct Payl {
    text: String,
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

struct Vtte;

impl Atom for Vtte {
    const KIND: FourCC = FourCC::new(b"vtte");

    fn decode_body<B: Buf>(_buf: &mut B) -> mp4_atom::Result<Self> {
        Ok(Self)
    }

    fn encode_body<B: BufMut>(&self, _buf: &mut B) -> mp4_atom::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{WvttFormat, WvttSample};
    use crate::packager::{MediaSegment, Packager, PackagerError, Sample};

    fn segment() -> MediaSegment<WvttSample> {
        MediaSegment::new(
            0,
            vec![Sample::new(1_000, WvttSample::new(vec!["cue".into()]))],
        )
    }

    #[test]
    fn package_rejects_a_zero_track_id() {
        let error = Packager::new(WvttFormat, 1_000)
            .with_track_id(0)
            .package(&[segment()])
            .unwrap_err();

        assert!(matches!(error, PackagerError::InvalidTrackId));
    }

    #[test]
    fn package_rejects_a_zero_timescale() {
        let error = Packager::new(WvttFormat, 0)
            .package(&[segment()])
            .unwrap_err();

        assert!(matches!(error, PackagerError::InvalidTimescale));
    }

    #[test]
    fn package_rejects_media_that_covers_no_time() {
        let error = Packager::new(WvttFormat, 1_000).package(&[]).unwrap_err();

        assert!(matches!(error, PackagerError::Empty));
    }
}
