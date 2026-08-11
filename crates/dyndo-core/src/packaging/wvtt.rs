use mp4_atom::{
    Atom, Buf, BufMut, Codec, Decode, Encode, FourCC, Ftyp, PlainText, Styp, VttC, Wvtt,
};

use super::packager::{Format, Packager};
use super::{MediaSegment, PackageError};

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
struct WvttFormat;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WvttPackager {
    inner: Packager<WvttFormat>,
}

impl WvttPackager {
    pub fn new(timescale: u32) -> Self {
        Self {
            inner: Packager::new(WvttFormat, timescale),
        }
    }

    pub fn with_track_id(mut self, track_id: u32) -> Self {
        self.inner = self.inner.with_track_id(track_id);
        self
    }

    pub fn track_id(&self) -> u32 {
        self.inner.track_id()
    }

    pub fn timescale(&self) -> u32 {
        self.inner.timescale()
    }

    pub fn package(&self, segments: &[MediaSegment<WvttSample>]) -> Result<Vec<u8>, PackageError> {
        self.inner.package(segments)
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
    use super::super::Sample;
    use super::{MediaSegment, PackageError, WvttPackager, WvttSample};

    fn segment() -> MediaSegment<WvttSample> {
        MediaSegment::new(
            0,
            vec![Sample::new(1_000, WvttSample::new(vec!["cue".into()]))],
        )
    }

    #[test]
    fn package_rejects_a_zero_track_id() {
        let error = WvttPackager::new(1_000)
            .with_track_id(0)
            .package(&[segment()])
            .unwrap_err();

        assert!(matches!(error, PackageError::InvalidTrackId));
    }

    #[test]
    fn package_rejects_a_zero_timescale() {
        let error = WvttPackager::new(0).package(&[segment()]).unwrap_err();

        assert!(matches!(error, PackageError::InvalidTimescale));
    }

    #[test]
    fn package_rejects_media_that_covers_no_time() {
        let error = WvttPackager::new(1_000).package(&[]).unwrap_err();

        assert!(matches!(error, PackageError::Empty));
    }
}
