#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("backend error: {0}")]
    Backend(String),

    #[error("no sidx box found in {0} — input must be CMAF with a segment index")]
    MissingSidx(String),

    #[error("no moov/track found in {0}")]
    MissingMoov(String),

    #[error("expected exactly one track in {path}, found {count}")]
    NotSingleTrack { path: String, count: usize },

    #[error("unsupported codec {codec:?} in {path}")]
    UnsupportedCodec { path: String, codec: String },

    #[error("malformed {box_type} box in {path}: {reason}")]
    MalformedBox {
        box_type: String,
        path: String,
        reason: String,
    },

    #[error("duplicate track id {0} — inputs are not uniquely distinguishable")]
    DuplicateTrackId(String),

    #[error("MPD serialization failed: {0}")]
    MpdSerialization(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sidx_message_names_the_file() {
        let e = Error::MissingSidx("a.mp4".into());
        assert!(e.to_string().contains("a.mp4"));
        assert!(e.to_string().contains("sidx"));
    }
}
