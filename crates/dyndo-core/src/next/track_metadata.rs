//! Media-specific track metadata.

use mp4_atom::{Codec as SampleEntry, FourCC, Moov};
use opendal::Operator;
use relative_path::RelativePath;
use serde::{Deserialize, Serialize};

use super::box_reader;
use super::codec::codec_from_moov;
use super::error::Error;
use super::format::Format;
use super::role::Role;

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
    pub async fn probe(op: &Operator, path: &RelativePath) -> Result<TrackMetadata, Error> {
        match Format::from_path(path)? {
            Format::Cmaf => {
                let boxes = box_reader::scan(op, path).await?;
                TrackMetadata::from_moov(&boxes.moov, path)
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

    fn from_moov(moov: &Moov, path: &RelativePath) -> Result<TrackMetadata, Error> {
        let Some(track) = moov.trak.first() else {
            return Err(Error::InvalidTrack {
                path: path.to_owned(),
                reason: "the movie box contains no media track".to_string(),
            });
        };
        if track.mdia.minf.stbl.stsd.codecs.is_empty() {
            return Err(Error::InvalidTrack {
                path: path.to_owned(),
                reason: "the sample description contains no sample entry".to_string(),
            });
        }

        let handler = moov.trak[0].mdia.hdlr.handler;
        let kind = if handler == FourCC::new(b"vide") {
            Kind::Video(VideoKind::from_moov(moov, path)?)
        } else if handler == FourCC::new(b"soun") {
            Kind::Audio(AudioKind::from_moov(moov, path)?)
        } else if handler == FourCC::new(b"text") {
            Kind::Text(TextKind::from_moov(moov))
        } else {
            return Err(Error::InvalidTrack {
                path: path.to_owned(),
                reason: format!("media handler `{handler}` is unsupported"),
            });
        };

        Ok(TrackMetadata {
            codec: Some(codec_from_moov(moov, path)?),
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
    fn from_moov(moov: &Moov, path: &RelativePath) -> Result<VideoKind, Error> {
        let visual = match sample_entry(moov) {
            SampleEntry::Avc1(entry) => &entry.visual,
            SampleEntry::Av01(entry) => &entry.visual,
            SampleEntry::Hvc1(entry) => &entry.visual,
            SampleEntry::Hev1(entry) => &entry.visual,
            _ => {
                return Err(Error::InvalidTrack {
                    path: path.to_owned(),
                    reason: "the video track has no supported visual sample entry".to_string(),
                });
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
    fn from_moov(moov: &Moov, path: &RelativePath) -> Result<AudioKind, Error> {
        let audio = match sample_entry(moov) {
            SampleEntry::Mp4a(entry) => &entry.audio,
            SampleEntry::Ac3(entry) => &entry.audio,
            SampleEntry::Eac3(entry) => &entry.audio,
            _ => {
                return Err(Error::InvalidTrack {
                    path: path.to_owned(),
                    reason: "the audio track has no supported audio sample entry".to_string(),
                });
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
