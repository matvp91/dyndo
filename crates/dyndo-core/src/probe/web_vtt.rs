use opendal::Operator;
use relative_path::RelativePath;

use super::ProbeError;
use crate::text::Subtitle;
use crate::track::kind::{TextKind, TimedTextKind};
use crate::track::timed_text::TimedTextTrack;

impl TimedTextTrack {
    pub(super) async fn probe_web_vtt(
        op: &Operator,
        path: &RelativePath,
        id: String,
        kind: TextKind,
    ) -> Result<Self, ProbeError> {
        let document = String::from_utf8(op.read(path.as_str()).await?.to_bytes().to_vec())
            .map_err(|error| {
                opendal::Error::new(opendal::ErrorKind::Unexpected, "invalid VTT text")
                    .set_source(error)
            })?;
        let subtitle = Subtitle::from_vtt_text(&document)?;
        Ok(Self::new(
            id,
            path.to_owned(),
            TimedTextKind::WebVtt(kind),
            subtitle,
        ))
    }
}
