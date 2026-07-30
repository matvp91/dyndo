//! An asset's media track.

use opendal::Operator;
use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::track_metadata::{Kind, TrackMetadata};
use crate::error::CoreError;

/// A track's identity, location, and media-specific metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// The representation identifier used by manifests and segment routes.
    pub id: String,
    /// The resolved storage path of the track.
    pub path: RelativePathBuf,
    /// The track's media type and media-specific fields.
    #[serde(flatten)]
    pub metadata: TrackMetadata,
}

impl Track {
    /// Probe a track and assign a deterministic ID from its normalized path.
    ///
    /// # Errors
    /// Returns any format, storage, parsing, container, or codec error from
    /// probing the track metadata.
    pub async fn probe(
        op: &Operator,
        path: impl Into<RelativePathBuf>,
    ) -> Result<Track, CoreError> {
        let path = path.into().normalize();
        let metadata = TrackMetadata::probe(op, &path).await?;
        let prefix = match &metadata.kind {
            Kind::Video(_) => "video",
            Kind::Audio(_) => "audio",
            Kind::Text(_) => "text",
        };
        let uuid_name = format!("dyndo:track:{}", path.as_str());
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, uuid_name.as_bytes());

        Ok(Track {
            id: format!("{prefix}_{uuid}"),
            path,
            metadata,
        })
    }
}
