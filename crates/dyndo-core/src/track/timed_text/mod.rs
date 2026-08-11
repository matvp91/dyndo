use relative_path::{RelativePath, RelativePathBuf};

use crate::text::Subtitle;
use crate::track::kind::TimedTextKind;

pub mod web_vtt;

/// A source track represented by timed-text documents.
#[derive(Clone)]
pub struct TimedTextTrack {
    id: String,
    path: RelativePathBuf,
    kind: TimedTextKind,
    subtitle: Subtitle,
}

impl TimedTextTrack {
    pub(crate) fn new(
        id: String,
        path: RelativePathBuf,
        kind: TimedTextKind,
        subtitle: Subtitle,
    ) -> Self {
        Self {
            id,
            path,
            kind,
            subtitle,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    pub fn kind(&self) -> &TimedTextKind {
        &self.kind
    }
}
