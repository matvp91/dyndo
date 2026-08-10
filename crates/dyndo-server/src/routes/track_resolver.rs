use std::ops::Range;

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::probe::probe_tracks;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use opendal::Operator;

use crate::error::ServerError;

pub(super) struct TrackResolver<'a> {
    operator: &'a Operator,
    asset: &'a AssetDescriptor,
}

pub(super) struct LocatedSegment {
    pub(super) track: Track,
    pub(super) byte_range: Range<u64>,
}

impl<'a> TrackResolver<'a> {
    pub(super) fn new(operator: &'a Operator, asset: &'a AssetDescriptor) -> Self {
        Self { operator, asset }
    }

    pub(super) async fn probe(&self, track_id: &str) -> Result<Track, ServerError> {
        let descriptor = self
            .asset
            .find_track_by_id(track_id)
            .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
        let path = self.asset.track_path(descriptor);

        Track::probe(
            self.operator,
            &path,
            Some(descriptor),
            &self.asset.segment_options,
        )
        .await
        .map_err(Into::into)
    }

    pub(super) async fn probe_all(&self) -> Result<Vec<Track>, ServerError> {
        probe_tracks(self.operator, self.asset)
            .await
            .map_err(Into::into)
    }

    pub(super) async fn locate_segment(
        &self,
        track_id: &str,
        time: u64,
    ) -> Result<LocatedSegment, ServerError> {
        let track = self.probe(track_id).await?;
        let byte_range = ServedSegment::group(
            track.segments(),
            self.asset.segment_options.min_length,
            &self.asset.segment_options.boundaries,
        )
        .into_iter()
        .find(|segment| segment.unscaled_start_time() == time)
        .map(|segment| segment.byte_range())
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;

        Ok(LocatedSegment { track, byte_range })
    }
}
