//! Errors produced by the next core model.

use relative_path::RelativePathBuf;

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
        reason: InvalidTrack,
    },
    /// No segment starts at the requested presentation time.
    #[error("no segment starts at {start}")]
    SegmentNotFound {
        /// The requested start time.
        start: u64,
    },
    /// A segment index cannot be grouped without a timescale.
    #[error("cannot group a segment index with a zero timescale")]
    ZeroSegmentTimescale,
    /// A grouped segment's duration does not fit in a `u64`.
    #[error("grouped segment duration overflows")]
    SegmentDurationOverflow,
    /// A segment interval does not align with CMAF source boundaries.
    #[error(
        "segment at {start} with duration {duration} does not align with CMAF segment boundaries"
    )]
    CmafRangeNotFound {
        /// The requested start time.
        start: u64,
        /// The requested duration.
        duration: u64,
    },
}

/// The reason a decoded track is invalid.
#[derive(Debug, thiserror::Error)]
pub enum InvalidTrack {
    /// A box does not declare its body size.
    #[error("a box has no declared size")]
    MissingBoxSize,
    /// No movie box occurs before the first media fragment.
    #[error("the movie box is missing before the first media fragment")]
    MissingMovieBox,
    /// No segment index occurs before the first media fragment.
    #[error("the segment index is missing before the first media fragment")]
    MissingSegmentIndex,
    /// The track has no media fragment.
    #[error("the first media fragment is missing")]
    MissingMediaFragment,
    /// The movie box contains no media track.
    #[error("the movie box contains no media track")]
    MissingMediaTrack,
    /// The sample description contains no sample entry.
    #[error("the sample description contains no sample entry")]
    MissingSampleEntry,
    /// The segment index has a zero timescale.
    #[error("the segment-index timescale is zero")]
    ZeroTimescale,
    /// The segment index points to another segment index.
    #[error("hierarchical segment indexes are unsupported")]
    HierarchicalSegmentIndex,
    /// Segment timing or byte-offset accumulation overflowed.
    #[error("segment-index timing or byte offset overflows")]
    SegmentIndexOverflow,
    /// A box ends before its declared body size.
    #[error("a box body is truncated")]
    TruncatedBox,
    /// The track uses a media handler dyndo does not support.
    #[error("media handler `{handler}` is unsupported")]
    UnsupportedMediaHandler {
        /// The handler's four-character code.
        handler: String,
    },
    /// The track uses a codec dyndo does not support.
    #[error("codec `{codec}` is unsupported")]
    UnsupportedCodec {
        /// The codec or sample-entry name.
        codec: String,
    },
    /// A video track has no compatible visual sample entry.
    #[error("the video track has no supported visual sample entry")]
    MissingVisualSampleEntry,
    /// An audio track has no compatible audio sample entry.
    #[error("the audio track has no supported audio sample entry")]
    MissingAudioSampleEntry,
}
