//! Errors produced by the next core model.

use relative_path::RelativePathBuf;

use super::segment::Segment;

/// An error produced while reading, writing, or probing dyndo data.
///
/// Each variant describes a dyndo operation or domain failure. Underlying
/// library errors are retained as sources while dyndo controls the displayed
/// message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An asset descriptor could not be read from storage.
    #[error("could not read asset descriptor `{path}`")]
    ReadDescriptor {
        /// The asset descriptor path.
        path: RelativePathBuf,
        /// The storage failure.
        #[source]
        source: opendal::Error,
    },
    /// An asset descriptor could not be decoded.
    #[error("could not decode asset descriptor `{path}`")]
    DecodeDescriptor {
        /// The asset descriptor path.
        path: RelativePathBuf,
        /// The JSON decoding failure.
        #[source]
        source: serde_json::Error,
    },
    /// An asset descriptor could not be encoded.
    #[error("could not encode asset descriptor `{path}`")]
    EncodeDescriptor {
        /// The asset descriptor path.
        path: RelativePathBuf,
        /// The JSON encoding failure.
        #[source]
        source: serde_json::Error,
    },
    /// An asset descriptor could not be written to storage.
    #[error("could not write asset descriptor `{path}`")]
    WriteDescriptor {
        /// The asset descriptor path.
        path: RelativePathBuf,
        /// The storage failure.
        #[source]
        source: opendal::Error,
    },
    /// A track path has no file extension.
    #[error("track `{path}` has no file extension")]
    MissingTrackExtension {
        /// The track path.
        path: RelativePathBuf,
    },
    /// A track path has an unsupported file extension.
    #[error("track `{path}` has unsupported format `{extension}`")]
    UnsupportedTrackFormat {
        /// The track path.
        path: RelativePathBuf,
        /// The unsupported extension.
        extension: String,
    },
    /// A track could not be opened in storage.
    #[error("could not open track `{path}`")]
    OpenTrack {
        /// The track path.
        path: RelativePathBuf,
        /// The storage failure.
        #[source]
        source: opendal::Error,
    },
    /// Track bytes could not be read from an open stream.
    #[error("could not read track `{path}`")]
    ReadTrack {
        /// The track path.
        path: RelativePathBuf,
        /// The stream I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// A track container could not be decoded.
    #[error("could not parse track container `{path}`")]
    ParseTrack {
        /// The track path.
        path: RelativePathBuf,
        /// The MP4 decoding failure.
        #[source]
        source: mp4_atom::Error,
    },
    /// A decoded track violates a semantic or structural requirement.
    #[error("invalid track `{path}`: {reason}")]
    InvalidTrack {
        /// The track path.
        path: RelativePathBuf,
        /// The violated requirement.
        reason: String,
    },
    /// Timed text could not be parsed.
    #[error("could not parse timed text `{path}`: {reason}")]
    ParseText {
        /// The timed-text path.
        path: RelativePathBuf,
        /// The reason parsing failed.
        reason: String,
    },
    /// No segment starts at the requested presentation time.
    #[error("no segment starts at {0}")]
    SegmentNotFound(
        /// The requested start time.
        u64,
    ),
    /// A segment index cannot be grouped without a timescale.
    #[error("cannot group a segment index with a zero timescale")]
    ZeroSegmentTimescale,
    /// A grouped segment's duration does not fit in a `u64`.
    #[error("grouped segment duration overflows")]
    SegmentDurationOverflow,
    /// No source byte range matches a segment interval.
    #[error("no byte range found for segment {0:?}")]
    RangeNotFound(
        /// The segment whose byte range was requested.
        Segment,
    ),
    /// An asset could not be serialized as a DASH MPD.
    #[error("could not serialize DASH MPD: {0}")]
    SerializeDash(String),
}
