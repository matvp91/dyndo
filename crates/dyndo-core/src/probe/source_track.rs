use opendal::Operator;
use relative_path::RelativePath;
use uuid::Uuid;

use super::ProbeError;
use crate::track::ResolvedSourceTrack;
use crate::track::SourceTrack;
use crate::track::cmaf::ResolvedCmafTrack;
use crate::track::kind::TimedTextKind;
use crate::track::kind::{TextKind, undetermined_language};
use crate::track::timed_text::ResolvedTimedTextTrack;

impl ResolvedSourceTrack {
    pub async fn probe(
        op: &Operator,
        path: &RelativePath,
        track: Option<&SourceTrack>,
    ) -> Result<Self, ProbeError> {
        match track {
            Some(SourceTrack::TimedText(track)) => match &track.kind {
                TimedTextKind::WebVtt(kind) => {
                    ResolvedTimedTextTrack::probe_web_vtt(op, path, track.id.clone(), kind.clone())
                        .await
                        .map(Self::TimedText)
                }
            },
            Some(SourceTrack::Cmaf(track)) => {
                ResolvedCmafTrack::probe(op, path, track.id.clone(), Some(track.kind.clone()))
                    .await
                    .map(Self::Cmaf)
            }
            None => {
                let id = source_id(path);
                if path.as_str().ends_with(".vtt") {
                    let kind = TextKind {
                        language: undetermined_language(),
                        role: None,
                    };
                    return ResolvedTimedTextTrack::probe_web_vtt(op, path, id, kind)
                        .await
                        .map(Self::TimedText);
                }
                ResolvedCmafTrack::probe(op, path, id, None)
                    .await
                    .map(Self::Cmaf)
            }
        }
    }
}

fn source_id(path: &RelativePath) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes()).to_string()
}
