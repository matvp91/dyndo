//! Reads byte ranges from the effective media representation.
//!
//! Plain WebVTT sources are packaged while they are read, so probing and serving
//! must use the same configured operator to agree on what each byte range means.

use std::ops::Range;

use bytes::Bytes;
use opendal::Operator;

use super::segment_options::SegmentOptions;
use super::track::Track;

use self::vtt_layer::VttLayer;

mod vtt_layer;

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct TrackReadError(#[from] opendal::Error);

pub struct Reader<'a> {
    op: Operator,
    track: &'a Track,
}

impl<'a> Reader<'a> {
    pub fn new(op: &Operator, track: &'a Track, options: &SegmentOptions) -> Self {
        Self {
            op: operator(op, options),
            track,
        }
    }

    pub async fn read_initialization(&self) -> Result<Bytes, TrackReadError> {
        self.read_range(self.track.init_segment().byte_range())
            .await
    }

    pub async fn read_range(&self, range: Range<u64>) -> Result<Bytes, TrackReadError> {
        Ok(self
            .op
            .read_with(self.track.path().as_str())
            .range(range)
            .await?
            .to_bytes())
    }
}

pub(crate) fn operator(op: &Operator, options: &SegmentOptions) -> Operator {
    op.clone()
        .layer(VttLayer::new(&options.boundaries, options.text_length))
}
