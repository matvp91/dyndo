use ::opendal::Operator;
use bytes::Bytes;
use futures_util::future::try_join_all;
use relative_path::RelativePath;
use uuid::Uuid;

use self::box_reader::BoxReaderError;
use self::metadata::{build_codec, build_kind};
use self::segment_index::{build_init_segment, build_segments};
use super::asset_descriptor::AssetDescriptor;
use super::cmaf_package::CmafPackage;
use super::cmaf_track::CmafTrack;
use super::cmaf_track_kind::{CmafTrackKind, TextKind, undetermined_language};
use super::segment_options::SegmentOptions;
use super::source_track::SourceTrack;
use super::text::Subtitle;
use super::track_descriptor::TrackDescriptor;
use super::web_vtt_track::WebVttTrack;

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
    #[error("asset descriptor entry is not a source track")]
    NotSourceTrack,
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error(transparent)]
    Vtt(#[from] super::text::vtt::VttParseError),
    #[error(transparent)]
    Package(#[from] super::packaging::PackageError),
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

impl CmafTrack {
    async fn probe(
        op: &Operator,
        path: &RelativePath,
        identity: Option<(String, CmafTrackKind)>,
    ) -> Result<Self, ProbeError> {
        let boxes = box_reader::scan(op, path.as_str()).await?;
        Self::from_boxes(boxes, path, identity)
    }

    async fn from_bytes(
        bytes: Bytes,
        path: &RelativePath,
        id: String,
        kind: CmafTrackKind,
    ) -> Result<Self, ProbeError> {
        let boxes = box_reader::scan_bytes(bytes).await?;
        Self::from_boxes(boxes, path, Some((id, kind)))
    }

    fn from_boxes(
        boxes: box_reader::Boxes,
        path: &RelativePath,
        identity: Option<(String, CmafTrackKind)>,
    ) -> Result<Self, ProbeError> {
        let init_segment = build_init_segment(&boxes, build_codec(&boxes)?);
        let probed_kind = build_kind(&boxes)?;
        let (id, kind) = match identity {
            Some(identity) => identity,
            None => (
                Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes()).to_string(),
                probed_kind,
            ),
        };
        let segments = build_segments(&boxes, &init_segment)?;

        Ok(Self::new(id, path.to_owned(), kind, init_segment, segments))
    }
}

impl WebVttTrack {
    async fn probe(
        op: &Operator,
        path: &RelativePath,
        id: String,
        kind: TextKind,
    ) -> Result<Self, ProbeError> {
        let document = String::from_utf8(op.read(path.as_str()).await?.to_bytes().to_vec())
            .map_err(|error| {
                opendal::Error::new(opendal::ErrorKind::Unexpected, "invalid VTT text")
                    .set_source(error)
            })?;
        let subtitle = Subtitle::from_vtt_text(&document)?;
        Ok(Self::new(id, path.to_owned(), kind, subtitle))
    }

    /// Builds the temporary CMAF representation required by a CMAF operation.
    pub async fn package(&self, options: &SegmentOptions) -> Result<CmafPackage, ProbeError> {
        let bytes = self.package_bytes(options)?;
        let cmaf = CmafTrack::from_bytes(
            bytes.clone(),
            self.path(),
            self.id().to_string(),
            CmafTrackKind::Text(self.kind().clone()),
        )
        .await?;
        Ok(CmafPackage::new(cmaf, bytes))
    }
}

impl SourceTrack {
    pub async fn probe(
        op: &Operator,
        path: &RelativePath,
        descriptor: Option<&TrackDescriptor>,
    ) -> Result<Self, ProbeError> {
        match descriptor {
            Some(TrackDescriptor::Vtt(descriptor)) => {
                WebVttTrack::probe(op, path, descriptor.id.clone(), descriptor.kind.clone())
                    .await
                    .map(Self::Vtt)
            }
            Some(TrackDescriptor::Thumbnail(_)) => Err(ProbeError::NotSourceTrack),
            descriptor => {
                if path.as_str().ends_with(".vtt") {
                    let id =
                        Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes()).to_string();
                    let kind = TextKind {
                        language: undetermined_language(),
                        role: None,
                    };
                    return WebVttTrack::probe(op, path, id, kind).await.map(Self::Vtt);
                }
                let identity = descriptor
                    .map(|descriptor| {
                        descriptor
                            .cmaf_kind()
                            .map(|kind| (descriptor.id().to_string(), kind))
                            .ok_or(ProbeError::NotSourceTrack)
                    })
                    .transpose()?;
                CmafTrack::probe(op, path, identity).await.map(Self::Cmaf)
            }
        }
    }
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
