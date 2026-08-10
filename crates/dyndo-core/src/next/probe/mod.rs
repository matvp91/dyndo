use ::opendal::Operator;
use futures_util::future::try_join_all;
use relative_path::RelativePath;
use uuid::Uuid;

use super::asset_descriptor::AssetDescriptor;
use super::reader::Reader;
use super::segment_options::SegmentOptions;
use super::track::Track;
use super::track_descriptor::TrackDescriptor;
use super::track_kind::TrackKind;

use self::box_reader::BoxReaderError;
use self::metadata::{build_codec, build_kind};
use self::segment_index::{build_init_segment, build_segments};

mod box_reader;
mod metadata;
mod segment_index;

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

impl Track {
    pub async fn probe(
        op: &Operator,
        path: &RelativePath,
        descriptor: Option<&TrackDescriptor>,
        options: &SegmentOptions,
    ) -> Result<Self, ProbeError> {
        let op = Reader::op(op, options);
        let boxes = box_reader::scan(&op, path.as_str()).await?;
        let init_segment = build_init_segment(&boxes, build_codec(&boxes)?);
        let probed_kind = build_kind(&boxes)?;
        let (id, kind) = match descriptor {
            Some(descriptor) => (descriptor.id.clone(), descriptor.kind.clone()),
            None => (generate_id(&probed_kind, path), probed_kind),
        };
        let segments = build_segments(&boxes, &init_segment)?;

        Ok(Self::new(id, path.to_owned(), kind, init_segment, segments))
    }
}

pub async fn probe_tracks(
    op: &Operator,
    asset: &AssetDescriptor,
) -> Result<Vec<Track>, ProbeError> {
    let probes = asset.tracks.iter().map(|descriptor| {
        let path = asset.track_path(descriptor);
        async move { Track::probe(op, &path, Some(descriptor), &asset.segment_options).await }
    });

    try_join_all(probes).await
}

fn generate_id(kind: &TrackKind, path: &RelativePath) -> String {
    let hash = Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes());

    format!("{}_{hash}", kind.content_type())
}
