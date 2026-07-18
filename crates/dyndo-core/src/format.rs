//! The container format of a track file: [`Format::from_path`] infers it
//! from the file extension, and the header and metadata reads
//! (`Header::read`, [`Metadata::read`](crate::metadata::Metadata::read))
//! dispatch on it.

use crate::error::CoreError;

/// The container format of a track file, inferred from its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A CMAF (fragmented MP4) track file.
    Cmaf,
    /// A raw WebVTT subtitle file.
    Vtt,
}

impl Format {
    /// Infer the format of the track file at `path` from its extension,
    /// case-insensitively.
    ///
    /// # Errors
    /// [`CoreError::UnsupportedFormat`] on any other extension.
    pub fn from_path(path: &str) -> Result<Format, CoreError> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("mp4") => Ok(Format::Cmaf),
            Some("vtt") => Ok(Format::Vtt),
            other => Err(CoreError::UnsupportedFormat(format!(
                "no format for file extension {other:?} (supported: mp4, vtt)"
            ))),
        }
    }
}
