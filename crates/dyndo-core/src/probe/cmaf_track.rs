use bytes::Bytes;
use opendal::Operator;
use relative_path::RelativePath;
use uuid::Uuid;

use super::ProbeError;
use super::box_reader;
use super::metadata::{build_codec, build_kind};
use super::segment_index::{build_init_segment, build_segments};
use crate::cmaf_track::CmafTrack;
use crate::cmaf_track_kind::CmafTrackKind;

impl CmafTrack {
    pub(super) async fn probe(
        op: &Operator,
        path: &RelativePath,
        identity: Option<(String, CmafTrackKind)>,
    ) -> Result<Self, ProbeError> {
        let boxes = box_reader::scan(op, path.as_str()).await?;
        Self::from_boxes(boxes, path, identity)
    }

    pub(crate) async fn from_bytes(
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
