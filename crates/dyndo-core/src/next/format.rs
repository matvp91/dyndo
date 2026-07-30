//! Track file formats inferred from file extensions.

use relative_path::RelativePath;

use super::error::Error;

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
    /// Returns an error when the path has no extension or its extension is not
    /// supported.
    pub fn from_path(path: &RelativePath) -> Result<Format, Error> {
        let extension = path.extension().map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("mp4") => Ok(Format::Cmaf),
            Some("vtt") => Ok(Format::Vtt),
            Some(extension) => Err(Error::UnsupportedTrackFormat {
                path: path.to_owned(),
                extension: extension.to_owned(),
            }),
            None => Err(Error::MissingTrackExtension {
                path: path.to_owned(),
            }),
        }
    }
}
