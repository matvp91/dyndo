use std::ops::Range;

use dyndo_core::asset_descriptor::AssetDescriptor;
use dyndo_core::cmaf_package::CmafPackage;
use dyndo_core::cmaf_track::CmafTrack;
use dyndo_core::probe::probe_source_tracks;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::source_track::SourceTrack;
use dyndo_core::vtt_track::VttTrack;
use opendal::Operator;

use crate::error::ServerError;

pub(super) struct TrackResolver<'a> {
    operator: &'a Operator,
    asset: &'a AssetDescriptor,
}

pub(super) struct LocatedSegment {
    pub(super) track: ResolvedTrack,
    pub(super) byte_range: Range<u64>,
    pub(super) start_time: u64,
    pub(super) end_time: u64,
}

pub(super) enum ResolvedTrack {
    Cmaf(CmafTrack),
    Vtt {
        source: VttTrack,
        packaged: CmafPackage,
    },
}

impl ResolvedTrack {
    pub(super) fn cmaf(&self) -> &CmafTrack {
        match self {
            Self::Cmaf(track) => track,
            Self::Vtt { packaged, .. } => packaged.cmaf(),
        }
    }

    pub(super) fn web_vtt(&self) -> Option<&VttTrack> {
        match self {
            Self::Vtt { source, .. } => Some(source),
            Self::Cmaf(_) => None,
        }
    }

    pub(super) fn packaged(&self) -> Option<&CmafPackage> {
        match self {
            Self::Vtt { packaged, .. } => Some(packaged),
            Self::Cmaf(_) => None,
        }
    }
}

impl<'a> TrackResolver<'a> {
    pub(super) fn new(operator: &'a Operator, asset: &'a AssetDescriptor) -> Self {
        Self { operator, asset }
    }

    pub(super) async fn probe(&self, track_id: &str) -> Result<SourceTrack, ServerError> {
        let descriptor = self
            .asset
            .find_track_by_id(track_id)
            .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
        let path = self
            .asset
            .track_path(descriptor)
            .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")));
        let path = path?;

        SourceTrack::probe(self.operator, &path, Some(descriptor))
            .await
            .map_err(Into::into)
    }

    pub(super) async fn probe_all(&self) -> Result<Vec<SourceTrack>, ServerError> {
        probe_source_tracks(self.operator, self.asset)
            .await
            .map_err(Into::into)
    }

    pub(super) async fn resolve(&self, track_id: &str) -> Result<ResolvedTrack, ServerError> {
        match self.probe(track_id).await? {
            SourceTrack::Cmaf(track) => Ok(ResolvedTrack::Cmaf(track)),
            SourceTrack::Vtt(source) => {
                let packaged = source.package(&self.asset.segment_options).await?;
                Ok(ResolvedTrack::Vtt { source, packaged })
            }
        }
    }

    pub(super) async fn locate_segment(
        &self,
        track_id: &str,
        time: u64,
    ) -> Result<LocatedSegment, ServerError> {
        let track = self.resolve(track_id).await?;
        let segment = ServedSegment::group(
            track.cmaf().segments(),
            self.asset.segment_options.min_length,
            &self.asset.segment_options.boundaries,
        )
        .into_iter()
        .find(|segment| segment.unscaled_start_time() == time)
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;
        let byte_range = segment.byte_range();
        let start_time = segment.start_time();
        let end_time = segment.end_time();

        Ok(LocatedSegment {
            track,
            byte_range,
            start_time,
            end_time,
        })
    }
}
