use std::ops::Range;

use bytes::Bytes;
use opendal::Operator;

use self::vtt_layer::VttLayer;
use super::segment_options::SegmentOptions;
use super::track::Track;

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
            op: Self::op(op, options),
            track,
        }
    }

    pub(crate) fn op(op: &Operator, options: &SegmentOptions) -> Operator {
        op.clone()
            .layer(VttLayer::new(&options.boundaries, options.text_length))
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
