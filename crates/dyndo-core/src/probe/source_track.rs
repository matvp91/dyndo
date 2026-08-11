use opendal::Operator;
use relative_path::RelativePath;
use uuid::Uuid;

use super::ProbeError;
use crate::asset::kind::{TextKind, undetermined_language};
use crate::asset::track::SourceTrackDescriptor;
use crate::track::SourceTrack;
use crate::track::cmaf::CmafTrack;
use crate::track::timed_text::TimedTextTrack;

impl SourceTrack {
    pub async fn probe(
        op: &Operator,
        path: &RelativePath,
        descriptor: Option<&SourceTrackDescriptor>,
    ) -> Result<Self, ProbeError> {
        match descriptor {
            Some(SourceTrackDescriptor::WebVtt(descriptor)) => TimedTextTrack::probe_web_vtt(
                op,
                path,
                descriptor.id.clone(),
                descriptor.kind.clone(),
            )
            .await
            .map(Self::TimedText),
            Some(descriptor) => {
                let kind = descriptor.cmaf_kind().ok_or(ProbeError::NotSourceTrack)?;
                CmafTrack::probe(op, path, descriptor.id().to_string(), Some(kind))
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
                    return TimedTextTrack::probe_web_vtt(op, path, id, kind)
                        .await
                        .map(Self::TimedText);
                }
                CmafTrack::probe(op, path, id, None).await.map(Self::Cmaf)
            }
        }
    }
}

fn source_id(path: &RelativePath) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes()).to_string()
}
