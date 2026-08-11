use opendal::Operator;
use relative_path::RelativePath;
use uuid::Uuid;

use super::ProbeError;
use crate::cmaf_track::CmafTrack;
use crate::cmaf_track_kind::{TextKind, undetermined_language};
use crate::source_track::SourceTrack;
use crate::track_descriptor::TrackDescriptor;
use crate::web_vtt_track::WebVttTrack;

impl SourceTrack {
    pub async fn probe(
        op: &Operator,
        path: &RelativePath,
        descriptor: Option<&TrackDescriptor>,
    ) -> Result<Self, ProbeError> {
        match descriptor {
            Some(TrackDescriptor::WebVtt(descriptor)) => {
                WebVttTrack::probe(op, path, descriptor.id.clone(), descriptor.kind.clone())
                    .await
                    .map(Self::WebVtt)
            }
            Some(TrackDescriptor::Thumbnail(_)) => Err(ProbeError::NotSourceTrack),
            descriptor => {
                if path.as_str().ends_with(".vtt") {
                    let id =
                        Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes()).to_string();
                    let kind = TextKind {
                        language: undetermined_language(),
                        role: None,
                    };
                    return WebVttTrack::probe(op, path, id, kind)
                        .await
                        .map(Self::WebVtt);
                }
                let identity = descriptor
                    .map(|descriptor| {
                        descriptor
                            .cmaf_kind()
                            .map(|kind| (descriptor.id().to_string(), kind))
                            .ok_or(ProbeError::NotSourceTrack)
                    })
                    .transpose()?;
                CmafTrack::probe(op, path, identity).await.map(Self::Cmaf)
            }
        }
    }
}
