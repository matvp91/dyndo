mod atom;

use mp4_atom::{
    Any, Atom, BufMut, Codec, DecodeMaybe, Encode, FourCC, Ftyp, PlainText, Styp, VttC, Wvtt,
};

use self::atom::{Payl, Vttc, Vtte};
use super::format::Format;
use super::packager::Packager;
use super::unpackager::Unpackager;
use super::{MediaSegment, PackageError, UnpackageError, UnpackagedMedia};

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

    fn read_sample(&self, bytes: &[u8]) -> mp4_atom::Result<WvttSample> {
        let mut cues = Vec::new();
        let mut buf = bytes;

        while let Some(atom) = Any::decode_maybe(&mut buf)? {
            let Any::Unknown(kind, body) = atom else {
                continue;
            };
            if kind == Vttc::KIND {
                cues.push(Vttc::decode_body(&mut body.as_slice())?.payl.text);
            }
        }

        Ok(WvttSample::new(cues))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WvttUnpackager {
    inner: Unpackager<WvttFormat>,
}

impl WvttUnpackager {
    pub fn new() -> Self {
        Self {
            inner: Unpackager::new(WvttFormat),
        }
    }

    pub fn unpackage(&self, bytes: &[u8]) -> Result<UnpackagedMedia<WvttSample>, UnpackageError> {
        self.inner.unpackage(bytes)
    }
}

impl Default for WvttUnpackager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::Sample;
    use super::{
        MediaSegment, PackageError, UnpackageError, WvttPackager, WvttSample, WvttUnpackager,
    };

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

    #[test]
    fn unpackage_rejects_media_without_a_movie_timescale() {
        let error = WvttUnpackager::new().unpackage(&[]).unwrap_err();

        assert!(matches!(error, UnpackageError::MissingTimescale));
    }
}
