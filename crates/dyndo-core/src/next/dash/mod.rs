//! MPEG-DASH MPD generation for the next core model.

mod adaptation_set_group;
mod build;
mod compact;

use opendal::Operator;
use serde::Serialize;

use super::asset::Asset;
use super::error::Error;

/// Build a static MPEG-DASH MPD from `asset`.
///
/// Segment indexes are read on demand to obtain timing and bandwidth.
/// Raw tracks are not advertised because they have no DASH media-segment
/// transport in the current packaging model. When `compact` is true, values
/// common to every Representation are inherited from their AdaptationSet.
///
/// # Errors
/// Returns an error when a segment index cannot be read, when descriptor data
/// cannot form a conforming Representation, or when XML serialization fails.
pub async fn generate_mpd(op: &Operator, asset: &Asset, compact: bool) -> Result<String, Error> {
    let mpd = build::build_mpd(op, asset, compact).await?;
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| Error::SerializeDash(error.to_string()))?;
    Ok(xml)
}
