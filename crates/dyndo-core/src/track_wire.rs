//! The descriptor (`asset.json`) serialization of a [`Track`]: the stored
//! fields plus the derived debug-only `fourcc`, recomputed from the probed
//! header on every write and ignored when a descriptor is read back. The
//! id goes on the wire verbatim — [`Track::read`] generates it at probe
//! time, so a descriptor write pins that value and later metadata edits
//! can no longer change it.

use serde::{Serialize, Serializer};

use crate::metadata::Metadata;
use crate::track::Track;

/// The wire shape of a [`Track`]: its stored fields in descriptor order,
/// then the derived debug field.
#[derive(Serialize)]
struct TrackWire<'a> {
    id: &'a str,
    path: &'a str,
    #[serde(flatten)]
    metadata: &'a Metadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    fourcc: Option<&'a str>,
}

/// Serialize through `TrackWire`, adding the derived debug field.
///
/// # Panics
/// If the track has not been probed: the derived fields read the header.
impl Serialize for Track {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TrackWire {
            id: &self.id,
            path: &self.path,
            metadata: &self.metadata,
            fourcc: self.sample_entry(),
        }
        .serialize(serializer)
    }
}
