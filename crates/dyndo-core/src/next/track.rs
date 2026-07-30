//! An asset's media track.

use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::Error;
use super::track_metadata::{Kind, TrackMetadata};

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
    /// Resolve and probe a descriptor-relative track path, then assign a
    /// deterministic ID from the normalized storage path.
    ///
    /// # Errors
    /// Returns any format, storage, parsing, container, or codec error from
    /// probing the track metadata.
    pub async fn probe(
        op: &Operator,
        path: &str,
        asset_descriptor_path: &RelativePath,
    ) -> Result<Track, Error> {
        let path = asset_descriptor_path
            .parent()
            .unwrap_or_else(|| RelativePath::new(""))
            .join(path)
            .normalize();
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
