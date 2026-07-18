//! The descriptor (`asset.json`) serialization of a [`Track`]: the stored
//! fields plus derived debug-only fields (`mime_type`, `codec`), recomputed
//! from the probed header on every write and ignored when a descriptor is
//! read back.

use serde::{Serialize, Serializer};

use crate::metadata::Metadata;
use crate::track::Track;

/// The wire shape of a [`Track`]: its stored fields in descriptor order,
/// then the derived debug fields.
#[derive(Serialize)]
struct TrackWire<'a> {
    id: &'a str,
    path: &'a str,
    #[serde(flatten)]
    metadata: &'a Metadata,
    mime_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    codec: Option<&'a str>,
}

/// Serialize through [`TrackWire`], adding the derived debug fields.
///
/// # Panics
/// If the track has not been probed: the derived fields read the header.
impl Serialize for Track {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TrackWire {
            id: &self.id,
            path: &self.path,
            metadata: &self.metadata,
            mime_type: self.mime_type(),
            codec: self.codec(),
        }
        .serialize(serializer)
    }
}
