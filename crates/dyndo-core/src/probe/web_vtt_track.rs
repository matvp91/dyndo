use opendal::Operator;
use relative_path::RelativePath;

use super::ProbeError;
use crate::cmaf_track_kind::TextKind;
use crate::text::Subtitle;
use crate::web_vtt_track::WebVttTrack;

impl WebVttTrack {
    pub(super) async fn probe(
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
        Ok(Self::new(id, path.to_owned(), kind, subtitle))
    }
}
