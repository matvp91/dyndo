use bytes::Bytes;
use opendal::Operator;
use relative_path::RelativePath;

use super::ProbeError;
use super::box_reader;
use super::metadata::{build_codec, build_kind};
use super::segment_index::{build_init_segment, build_segments};
use crate::track::cmaf::ResolvedCmafTrack;
use crate::track::kind::CmafTrackKind;

impl ResolvedCmafTrack {
    pub(super) async fn probe(
        op: &Operator,
        path: &RelativePath,
        id: String,
        kind: Option<CmafTrackKind>,
    ) -> Result<Self, ProbeError> {
        let boxes = box_reader::scan(op, path.as_str()).await?;
        Self::from_boxes(boxes, path, id, kind)
    }

    pub(crate) async fn from_bytes(
        bytes: Bytes,
        path: &RelativePath,
        id: String,
        kind: CmafTrackKind,
    ) -> Result<Self, ProbeError> {
        let boxes = box_reader::scan_bytes(bytes).await?;
        Self::from_boxes(boxes, path, id, Some(kind))
    }

    fn from_boxes(
        boxes: box_reader::Boxes,
        path: &RelativePath,
        id: String,
        kind: Option<CmafTrackKind>,
    ) -> Result<Self, ProbeError> {
        let init_segment = build_init_segment(&boxes, build_codec(&boxes)?);
        let probed_kind = build_kind(&boxes)?;
        let segments = build_segments(&boxes, &init_segment)?;

        Ok(Self::new(
            id,
            path.to_owned(),
            kind.unwrap_or(probed_kind),
            init_segment,
            segments,
        ))
    }
}
