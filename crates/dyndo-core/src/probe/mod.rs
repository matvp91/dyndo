use ::opendal::Operator;
use futures_util::future::try_join_all;

use self::box_reader::BoxReaderError;
use super::asset::Asset;
use super::track::ResolvedSourceTrack;

mod box_reader;
mod cmaf_track;
mod metadata;
mod segment_index;
mod source_track;
mod web_vtt;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error(transparent)]
    BoxReader(#[from] BoxReaderError),
    #[error("unsupported video sample entry")]
    UnsupportedVideoSampleEntry,
    #[error("unsupported audio sample entry")]
    UnsupportedAudioSampleEntry,
    #[error("unsupported track handler")]
    UnsupportedTrackHandler,
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Vtt(#[from] super::text::vtt::VttParseError),
    #[error("video track has no sample duration")]
    MissingFrameRate,
    #[error("unsupported codec {0}")]
    UnsupportedCodec(String),
    #[error("segment offset overflows")]
    SegmentOffsetOverflow,
    #[error("segment range overflows")]
    SegmentRangeOverflow,
    #[error("segment time overflows")]
    SegmentTimeOverflow,
}

pub async fn probe_source_tracks(
    op: &Operator,
    asset: &Asset,
) -> Result<Vec<ResolvedSourceTrack>, ProbeError> {
    let probes = asset.source_tracks().map(|track| {
        let path = asset.track_path(track);
        async move { ResolvedSourceTrack::probe(op, &path, Some(track)).await }
    });

    try_join_all(probes).await
}
