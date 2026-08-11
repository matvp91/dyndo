use std::ops::Range;

use dyndo_core::asset::Asset;
use dyndo_core::track::ResolvedSourceTrack;
use dyndo_core::track::cmaf::{ResolvedCmafTrack, ServedSegment};
use dyndo_core::track::timed_text::ResolvedTimedTextTrack;
use opendal::Operator;

use crate::error::ServerError;

pub(super) struct TrackResolver<'a> {
    operator: &'a Operator,
    asset: &'a Asset,
}

pub(super) struct LocatedSegment {
    pub(super) track: RequestTrack,
    pub(super) byte_range: Range<u64>,
    pub(super) start_time: u64,
    pub(super) end_time: u64,
}

pub(super) enum RequestTrack {
    Cmaf(ResolvedCmafTrack),
    TimedText {
        source: ResolvedTimedTextTrack,
        cmaf: ResolvedCmafTrack,
    },
}

impl RequestTrack {
    pub(super) fn cmaf(&self) -> &ResolvedCmafTrack {
        match self {
            Self::Cmaf(track) => track,
            Self::TimedText { cmaf, .. } => cmaf,
        }
    }

    pub(super) fn is_web_vtt(&self) -> bool {
        match self {
            Self::TimedText { source, .. } => source.format().is_web_vtt(),
            Self::Cmaf(_) => false,
        }
    }

    pub(super) fn web_vtt_segment(&self, start: u64, end: u64) -> Option<String> {
        match self {
            Self::TimedText { source, .. } => source.web_vtt_segment(start, end),
            Self::Cmaf(_) => None,
        }
    }
}

impl<'a> TrackResolver<'a> {
    pub(super) fn new(operator: &'a Operator, asset: &'a Asset) -> Self {
        Self { operator, asset }
    }

    pub(super) async fn resolve_source(
        &self,
        track_id: &str,
    ) -> Result<ResolvedSourceTrack, ServerError> {
        let source = self
            .asset
            .find_source_track_by_id(track_id)
            .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
        let path = self.asset.track_path(source);

        source
            .resolve(self.operator, &path)
            .await
            .map_err(Into::into)
    }

    pub(super) async fn resolve_sources(&self) -> Result<Vec<ResolvedSourceTrack>, ServerError> {
        self.asset
            .resolve_sources(self.operator)
            .await
            .map_err(Into::into)
    }

    pub(super) async fn resolve(&self, track_id: &str) -> Result<RequestTrack, ServerError> {
        match self.resolve_source(track_id).await? {
            ResolvedSourceTrack::Cmaf(track) => Ok(RequestTrack::Cmaf(track)),
            ResolvedSourceTrack::TimedText(source) => {
                let cmaf = source.package_wvtt(&self.asset.segment_options).await?;
                Ok(RequestTrack::TimedText { source, cmaf })
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
