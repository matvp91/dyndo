use std::ops::Range;

use bytes::Bytes;
use opendal::Operator;

use super::track::cmaf::CmafTrack;

#[derive(Debug, thiserror::Error)]
pub enum TrackReadError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
}

pub struct Reader {
    op: Operator,
}

impl Reader {
    pub fn new(op: &Operator) -> Self {
        Self { op: op.clone() }
    }

    pub async fn read_initialization(&self, track: &CmafTrack) -> Result<Bytes, TrackReadError> {
        self.read_range(track, track.init_segment().byte_range())
            .await
    }

    pub async fn read_range(
        &self,
        track: &CmafTrack,
        range: Range<u64>,
    ) -> Result<Bytes, TrackReadError> {
        Ok(self
            .op
            .read_with(track.path().as_str())
            .range(range)
            .await?
            .to_bytes())
    }
}
