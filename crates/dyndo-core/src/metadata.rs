//! A track's per-media-type metadata: [`Metadata`] tags a track video,
//! audio, or text, and carries the fields specific to that type.
//! [`Metadata::read`] reads the metadata a track file declares in-band.

use mp4_atom::{Codec, FourCC, Moov};
use opendal::Operator;
use serde::{Deserialize, Serialize};

use crate::box_reader;
use crate::error::CoreError;
use crate::format::Format;
use crate::role::{AudioRole, TextRole};

/// A track's per-media-type fields, tagging the track video, audio, or text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Metadata {
    /// A video track's fields.
    Video(VideoMetadata),
    /// An audio track's fields.
    Audio(AudioMetadata),
    /// A timed-text track's fields.
    Text(TextMetadata),
}

impl Metadata {
    /// Read the metadata the track file at `path` declares in-band,
    /// dispatching on its [`Format`]. `role` is never read from the file:
    /// it is descriptor-declared only.
    ///
    /// # Errors
    /// [`CoreError::UnsupportedFormat`] if `path`'s extension maps to no
    /// supported format; [`CoreError::Storage`]/[`CoreError::Io`] if the
    /// object cannot be read; [`CoreError::Parse`]/[`CoreError::Container`]
    /// if a box cannot be decoded, a required box is missing or empty, or
    /// the media handler is unrecognized; [`CoreError::UnsupportedCodec`]
    /// if the handler and sample entry don't line up.
    pub async fn read(op: &Operator, path: &str) -> Result<Metadata, CoreError> {
        match Format::from_path(path)? {
            Format::Cmaf => {
                let boxes = box_reader::scan(op, path).await?;
                Metadata::from_moov(&boxes.moov)
            }
            // A raw VTT file declares no metadata in-band: it is timed text
            // with an undeclared language until the descriptor says
            // otherwise.
            Format::Vtt => Ok(Metadata::Text(TextMetadata {
                language: und(),
                role: None,
            })),
        }
    }

    /// Construct the [`Metadata`] a track's `moov` declares, dispatching on
    /// its media handler (`vide`, `soun`, or `text`).
    fn from_moov(moov: &Moov) -> Result<Metadata, CoreError> {
        let handler = moov.trak[0].mdia.hdlr.handler;
        if handler == FourCC::new(b"vide") {
            Ok(Metadata::Video(VideoMetadata::from_moov(moov)?))
        } else if handler == FourCC::new(b"soun") {
            Ok(Metadata::Audio(AudioMetadata::from_moov(moov)?))
        } else if handler == FourCC::new(b"text") {
            Ok(Metadata::Text(TextMetadata::from_moov(moov)))
        } else {
            Err(CoreError::Container(format!(
                "unrecognized media handler {handler}"
            )))
        }
    }
}

/// The video-specific fields of a [`Track`](crate::track::Track).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoMetadata {
    /// Visual width, in pixels.
    pub width: u32,
    /// Visual height, in pixels.
    pub height: u32,
}

impl VideoMetadata {
    /// The video fields the track's `moov` declares.
    fn from_moov(moov: &Moov) -> Result<VideoMetadata, CoreError> {
        let visual = match sample_entry(moov) {
            Codec::Avc1(a) => &a.visual,
            Codec::Av01(a) => &a.visual,
            Codec::Hvc1(a) => &a.visual,
            Codec::Hev1(a) => &a.visual,
            _ => {
                return Err(CoreError::UnsupportedCodec(
                    "video track without a supported visual sample entry".into(),
                ));
            }
        };
        Ok(VideoMetadata {
            width: visual.width as u32,
            height: visual.height as u32,
        })
    }
}

/// The audio-specific fields of a [`Track`](crate::track::Track).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMetadata {
    /// Sampling rate, in Hz.
    pub sample_rate: u32,
    /// Number of audio channels (e.g. 2 for stereo, 6 for 5.1).
    pub channels: u16,
    /// ISO-639-2 language code; `"und"` when neither the file nor the
    /// descriptor declares one, so consumers can rely on it being filled.
    #[serde(default = "und")]
    pub language: String,
    /// The track's purpose, if declared. Omitted when `None`. Never probed
    /// from the CMAF file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AudioRole>,
}

impl AudioMetadata {
    /// The audio fields the track's `moov` declares.
    fn from_moov(moov: &Moov) -> Result<AudioMetadata, CoreError> {
        let audio = match sample_entry(moov) {
            Codec::Mp4a(a) => &a.audio,
            Codec::Ac3(a) => &a.audio,
            Codec::Eac3(a) => &a.audio,
            _ => {
                return Err(CoreError::UnsupportedCodec(
                    "audio track without a supported audio sample entry".into(),
                ));
            }
        };
        Ok(AudioMetadata {
            sample_rate: audio.sample_rate.integer() as u32,
            channels: audio.channel_count,
            language: language(moov).unwrap_or_else(und),
            role: None,
        })
    }
}

/// The text-specific fields of a [`Track`](crate::track::Track).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMetadata {
    /// ISO-639-2 language code; `"und"` when neither the file nor the
    /// descriptor declares one, so consumers can rely on it being filled.
    #[serde(default = "und")]
    pub language: String,
    /// The track's purpose, if declared. Omitted when `None`. Never probed
    /// from the CMAF file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TextRole>,
}

impl TextMetadata {
    /// The text fields the track's `moov` declares.
    fn from_moov(moov: &Moov) -> TextMetadata {
        TextMetadata {
            language: language(moov).unwrap_or_else(und),
            role: None,
        }
    }
}

/// The track's sample entry, naming its codec.
fn sample_entry(moov: &Moov) -> &Codec {
    &moov.trak[0].mdia.minf.stbl.stsd.codecs[0]
}

/// The track's `mdhd` language, or `None` when the box leaves it empty.
fn language(moov: &Moov) -> Option<String> {
    let lang = moov.trak[0].mdia.mdhd.language.as_str();
    (!lang.is_empty()).then(|| lang.to_string())
}

/// The undetermined ISO-639-2 language code, the default when neither the
/// file nor the descriptor declares one.
fn und() -> String {
    "und".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_language_defaults_to_und_on_the_wire() {
        let m: Metadata =
            serde_json::from_str(r#"{"type":"audio","sample_rate":48000,"channels":2}"#)
                .expect("valid audio metadata without language");
        let Metadata::Audio(a) = m else {
            panic!("expected audio metadata");
        };
        assert_eq!(a.language, "und");
    }

    #[test]
    fn text_language_defaults_to_und_on_the_wire() {
        let m: Metadata = serde_json::from_str(r#"{"type":"text"}"#)
            .expect("valid text metadata without language");
        let Metadata::Text(t) = m else {
            panic!("expected text metadata");
        };
        assert_eq!(t.language, "und");
    }
}
