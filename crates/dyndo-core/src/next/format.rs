//! Track file formats inferred from file extensions.

use relative_path::RelativePath;

use crate::error::CoreError;

/// The container format of a track file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A CMAF fragmented MP4 track.
    Cmaf,
    /// A raw WebVTT track.
    Vtt,
}

impl Format {
    /// Infer a track's format from its file extension, case-insensitively.
    ///
    /// # Errors
    /// Returns [`CoreError::UnsupportedFormat`] for unsupported extensions.
    pub fn from_path(path: &RelativePath) -> Result<Format, CoreError> {
        let extension = path.extension().map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("mp4") => Ok(Format::Cmaf),
            Some("vtt") => Ok(Format::Vtt),
            other => Err(CoreError::UnsupportedFormat(format!(
                "no format for file extension {other:?} (supported: mp4, vtt)"
            ))),
        }
    }
}
