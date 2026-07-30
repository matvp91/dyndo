//! Media-specific track metadata.

use mp4_atom::{Codec as SampleEntry, FourCC, Moov};
use opendal::Operator;
use relative_path::RelativePath;
use serde::{Deserialize, Serialize};

use super::box_reader;
use super::codec::codec_from_moov;
use super::format::Format;
use super::role::Role;
use crate::error::CoreError;

/// Media-specific track metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMetadata {
    /// The track's codec, or `None` for a raw track without a codec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    /// The track's purpose, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// The track's media type and type-specific fields.
    #[serde(flatten)]
    pub kind: Kind,
}

impl TrackMetadata {
    /// Probe the media metadata declared by the track at `path`.
    ///
    /// # Errors
    /// Returns an error when the format is unsupported, storage cannot be
    /// read, or the CMAF box structure or codec is invalid.
    pub async fn probe(op: &Operator, path: &RelativePath) -> Result<TrackMetadata, CoreError> {
        match Format::from_path(path)? {
            Format::Cmaf => {
                let boxes = box_reader::scan(op, path.as_str()).await?;
                TrackMetadata::from_moov(&boxes.moov)
            }
            Format::Vtt => Ok(TrackMetadata {
                codec: None,
                role: None,
                kind: Kind::Text(TextKind {
                    language: "und".to_string(),
                }),
            }),
        }
    }

    fn from_moov(moov: &Moov) -> Result<TrackMetadata, CoreError> {
        let handler = moov.trak[0].mdia.hdlr.handler;
        let kind = if handler == FourCC::new(b"vide") {
            Kind::Video(VideoKind::from_moov(moov)?)
        } else if handler == FourCC::new(b"soun") {
            Kind::Audio(AudioKind::from_moov(moov)?)
        } else if handler == FourCC::new(b"text") {
            Kind::Text(TextKind::from_moov(moov))
        } else {
            return Err(CoreError::Container(format!(
                "unrecognized media handler {handler}"
            )));
        };

        Ok(TrackMetadata {
            codec: Some(codec_from_moov(moov)?),
            role: None,
            kind,
        })
    }
}

/// A track's media type and type-specific metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    /// Video track metadata.
    Video(VideoKind),
    /// Audio track metadata.
    Audio(AudioKind),
    /// Timed-text track metadata.
    Text(TextKind),
}

/// Video-specific track metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoKind {
    /// Visual width, in pixels.
    pub width: u32,
    /// Visual height, in pixels.
    pub height: u32,
}

impl VideoKind {
    fn from_moov(moov: &Moov) -> Result<VideoKind, CoreError> {
        let visual = match sample_entry(moov) {
            SampleEntry::Avc1(entry) => &entry.visual,
            SampleEntry::Av01(entry) => &entry.visual,
            SampleEntry::Hvc1(entry) => &entry.visual,
            SampleEntry::Hev1(entry) => &entry.visual,
            _ => {
                return Err(CoreError::UnsupportedCodec(
                    "video track without a supported visual sample entry".into(),
                ));
            }
        };

        Ok(VideoKind {
            width: visual.width as u32,
            height: visual.height as u32,
        })
    }
}

/// Audio-specific track metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioKind {
    /// Sampling rate, in Hz.
    pub sample_rate: u32,
    /// Number of audio channels.
    pub channels: u16,
    /// ISO-639-2 language code.
    pub language: String,
}

impl AudioKind {
    fn from_moov(moov: &Moov) -> Result<AudioKind, CoreError> {
        let audio = match sample_entry(moov) {
            SampleEntry::Mp4a(entry) => &entry.audio,
            SampleEntry::Ac3(entry) => &entry.audio,
            SampleEntry::Eac3(entry) => &entry.audio,
            _ => {
                return Err(CoreError::UnsupportedCodec(
                    "audio track without a supported audio sample entry".into(),
                ));
            }
        };

        Ok(AudioKind {
            sample_rate: audio.sample_rate.integer() as u32,
            channels: audio.channel_count,
            language: language(moov).unwrap_or_else(|| "und".to_string()),
        })
    }
}

/// Timed-text-specific track metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextKind {
    /// ISO-639-2 language code.
    pub language: String,
}

impl TextKind {
    fn from_moov(moov: &Moov) -> TextKind {
        TextKind {
            language: language(moov).unwrap_or_else(|| "und".to_string()),
        }
    }
}

fn sample_entry(moov: &Moov) -> &SampleEntry {
    &moov.trak[0].mdia.minf.stbl.stsd.codecs[0]
}

fn language(moov: &Moov) -> Option<String> {
    let language = moov.trak[0].mdia.mdhd.language.as_str();
    (!language.is_empty()).then(|| language.to_string())
}
