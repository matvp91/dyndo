use ::opendal::Operator;
use futures_util::future::try_join_all;

use self::box_reader::BoxReaderError;
use super::asset::AssetDescriptor;
use super::track::SourceTrack;

mod box_reader;
mod cmaf_track;
mod metadata;
mod segment_index;
mod source_track;
mod web_vtt_track;

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
    #[error("asset descriptor entry is not a source track")]
    NotSourceTrack,
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
    asset: &AssetDescriptor,
) -> Result<Vec<SourceTrack>, ProbeError> {
    let probes = asset.source_tracks().filter_map(|descriptor| {
        let path = asset.track_path(descriptor)?;
        Some(async move { SourceTrack::probe(op, &path, Some(descriptor)).await })
    });

    try_join_all(probes).await
}
