//! The `Asset` is the descriptor (`asset.json`) model: it deserializes from
//! and serializes to the wire directly, with runtime-only fields skipped.

use bytes::Buf;
use futures_util::future::try_join_all;
use opendal::Operator;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::track::Track;

/// A dyndo asset: its tracks and where the descriptor was sourced from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Asset {
    /// Minimum length of a served segment, in milliseconds (wire:
    /// `min_segment_length`). `0` (or an absent field — deserialization
    /// defaults) serves each CMAF fragment as its own segment.
    #[serde(
        rename = "min_segment_length",
        default,
        skip_serializing_if = "is_zero"
    )]
    pub min_segment_length_ms: u64,
    /// Splice points, in milliseconds from the start of the presentation
    /// (wire: `segment_boundaries`). Served segments never span one. Treated
    /// as a set: order and duplicates don't matter.
    #[serde(
        rename = "segment_boundaries",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub segment_boundaries_ms: Vec<u64>,
    /// The asset's tracks, in descriptor order.
    pub tracks: Vec<Track>,
    /// Path of the source descriptor (`asset.json`), used to resolve each
    /// track's relative path. Never on the wire; set by [`Asset::read`].
    #[serde(skip)]
    pub path: String,
}

impl Asset {
    /// An empty asset: no tracks, empty source path.
    pub fn new() -> Asset {
        Asset::default()
    }

    /// Read and deserialize the descriptor JSON at `path` through `op`,
    /// recording `path` as the asset's source. Track headers are left
    /// unprobed: call [`Asset::read_with_headers`] when the whole asset's
    /// geometry is needed, or [`Track::read_header`] on a single track when
    /// serving one representation. Each track's id comes verbatim from the
    /// descriptor, which carries it from index time — consumers key by
    /// [`Track::id`] directly.
    ///
    /// # Errors
    /// [`CoreError::Storage`] if the object is missing or unreadable;
    /// [`CoreError::Descriptor`] if the bytes are not valid descriptor JSON;
    /// [`CoreError::InvalidDescriptor`] if a track carries an empty id.
    pub async fn read(op: &Operator, path: &str) -> Result<Asset, CoreError> {
        let buf = op.read(path).await?;
        let mut asset: Asset = serde_json::from_reader(buf.reader())?;
        asset.path = path.to_string();
        // The id is the manifest/segment-route key and is never regenerated
        // on read, so a blank one would key silently by the empty string.
        // Reject it here rather than propagate it.
        if let Some(t) = asset.tracks.iter().find(|t| t.id.is_empty()) {
            return Err(CoreError::InvalidDescriptor(format!(
                "track {:?} has an empty id",
                t.path
            )));
        }
        Ok(asset)
    }

    /// Read the descriptor via [`Asset::read`] and probe every track's header,
    /// so the asset carries the geometry manifest generation and whole-asset
    /// segment access depend on. Prefer [`Asset::read`] plus a single
    /// [`Track::read_header`] when only one representation is served.
    ///
    /// # Errors
    /// Any [`CoreError`] from [`Asset::read`], or from probing a track's
    /// header.
    pub async fn read_with_headers(op: &Operator, path: &str) -> Result<Asset, CoreError> {
        let mut asset = Asset::read(op, path).await?;
        // Tracks are independent, so all headers are probed concurrently;
        // each read resolves the track's descriptor-relative path itself.
        try_join_all(asset.tracks.iter_mut().map(|t| t.read_header(op, path))).await?;
        Ok(asset)
    }

    /// Probe the header of the track advertised as `id` and return its index,
    /// or `None` if no track matches. Returns the index rather than a
    /// `&Track`: probing borrows `self` mutably, and that borrow must be
    /// released before the caller re-borrows the track shared — alongside
    /// asset-level fields like [`Asset::segment_boundaries_ms`]. A borrow
    /// cannot survive that mut→shared handoff, but an index can.
    ///
    /// # Errors
    /// Any [`CoreError`] from probing the matched track's header.
    pub async fn find_track_index(
        &mut self,
        op: &Operator,
        id: &str,
    ) -> Result<Option<usize>, CoreError> {
        let Some(idx) = self.tracks.iter().position(|t| t.id == id) else {
            return Ok(None);
        };
        self.tracks[idx].read_header(op, &self.path).await?;
        Ok(Some(idx))
    }

    /// Serialize to pretty JSON and write to `path` through `op`.
    ///
    /// # Errors
    /// [`CoreError::Descriptor`] if serialization fails; [`CoreError::Storage`]
    /// if the write fails.
    pub async fn write(&self, op: &Operator, path: &str) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec_pretty(self)?;
        op.write(path, bytes).await?;
        Ok(())
    }

    /// Read the track file at `path` — relative to this asset's descriptor
    /// (`self.path`) — through `op`, append it to the asset's tracks, and
    /// return it so descriptor-declared fields (language, role) can be
    /// adjusted before the asset is written. The track is returned unnamed:
    /// the caller names it via [`Track::generate_id`] once those fields are
    /// settled, so the id reflects the track's final initial metadata.
    ///
    /// # Errors
    /// [`CoreError::UnsupportedFormat`] if `path`'s extension maps to no
    /// supported format; otherwise any [`CoreError`] from reading or parsing
    /// the track file.
    pub async fn add_track(&mut self, op: &Operator, path: &str) -> Result<&mut Track, CoreError> {
        let track = Track::read(op, &self.path, path).await?;
        self.tracks.push(track);
        Ok(self.tracks.last_mut().expect("a track was just pushed"))
    }

    /// The asset's presentation duration, in milliseconds: the longest
    /// track's duration.
    ///
    /// # Panics
    /// If a track has not been probed.
    pub fn duration_ms(&self) -> u64 {
        self.tracks
            .iter()
            .map(Track::duration_ms)
            .max()
            .unwrap_or(0)
    }

    /// The longest served (sub)segment across the asset's tracks, in
    /// milliseconds (`0` if it has none), under the asset's grouping policy.
    ///
    /// # Panics
    /// If a track has not been probed.
    pub fn max_segment_duration_ms(&self) -> u64 {
        self.tracks
            .iter()
            .map(|t| {
                t.max_segment_duration_ms(&self.segment_boundaries_ms, self.min_segment_length_ms)
            })
            .max()
            .unwrap_or(0)
    }
}

/// `skip_serializing_if` helper: the wire omits a zero `min_segment_length`.
fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[cfg(test)]
mod tests {
    use opendal::services::Fs;

    use super::*;
    use crate::metadata::Metadata;

    /// An empty asset rooted at a tempdir holding one raw `subs.vtt`, plus
    /// the operator to read it through.
    fn asset_over_vtt(dir: &std::path::Path) -> (Operator, Asset) {
        std::fs::write(dir.join("subs.vtt"), "WEBVTT\n").unwrap();
        let op = Operator::new(Fs::default().root(dir.to_str().unwrap())).unwrap();
        let asset = Asset {
            path: "asset.json".to_string(),
            ..Asset::new()
        };
        (op, asset)
    }

    /// A tempdir holding a raw `subs.vtt` and a one-track `asset.json`
    /// descriptor advertising it as `text_und`, plus the operator to read it.
    fn descriptor_over_vtt(dir: &std::path::Path) -> Operator {
        std::fs::write(dir.join("subs.vtt"), "WEBVTT\n").unwrap();
        std::fs::write(
            dir.join("asset.json"),
            r#"{"tracks":[{"id":"text_und","path":"subs.vtt","type":"text"}]}"#,
        )
        .unwrap();
        Operator::new(Fs::default().root(dir.to_str().unwrap())).unwrap()
    }

    #[tokio::test]
    async fn find_track_index_probes_the_matched_track() {
        let dir = tempfile::tempdir().unwrap();
        let op = descriptor_over_vtt(dir.path());
        let mut asset = Asset::read(&op, "asset.json").await.unwrap();

        let idx = asset.find_track_index(&op, "text_und").await.unwrap();

        assert_eq!(idx, Some(0));
        // The header is now probed: the accessor no longer panics.
        assert_eq!(asset.tracks[0].mime_type(), "text/vtt");
    }

    #[tokio::test]
    async fn find_track_index_is_none_for_an_unknown_id() {
        let dir = tempfile::tempdir().unwrap();
        let op = descriptor_over_vtt(dir.path());
        let mut asset = Asset::read(&op, "asset.json").await.unwrap();

        let idx = asset.find_track_index(&op, "nope").await.unwrap();

        assert!(idx.is_none());
    }

    #[tokio::test]
    async fn add_track_probes_and_appends_the_track() {
        let dir = tempfile::tempdir().unwrap();
        let (op, mut asset) = asset_over_vtt(dir.path());

        asset.add_track(&op, "subs.vtt").await.unwrap();

        assert_eq!(asset.tracks.len(), 1);
        assert_eq!(asset.tracks[0].path, "subs.vtt");
    }

    #[tokio::test]
    async fn add_track_returns_the_appended_track_for_adjustment() {
        let dir = tempfile::tempdir().unwrap();
        let (op, mut asset) = asset_over_vtt(dir.path());

        let track = asset.add_track(&op, "subs.vtt").await.unwrap();
        let Metadata::Text(t) = &mut track.metadata else {
            panic!("a .vtt probes as text");
        };
        t.language = "eng".to_string();

        let Metadata::Text(t) = &asset.tracks[0].metadata else {
            panic!("a .vtt probes as text");
        };
        assert_eq!(t.language, "eng");
    }

    #[tokio::test]
    async fn add_track_surfaces_an_unsupported_extension() {
        let dir = tempfile::tempdir().unwrap();
        let (op, mut asset) = asset_over_vtt(dir.path());

        let err = asset.add_track(&op, "subs.srt").await.unwrap_err();

        assert!(matches!(err, CoreError::UnsupportedFormat(_)));
        assert!(asset.tracks.is_empty());
    }

    #[tokio::test]
    async fn read_rejects_a_track_with_an_empty_id() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("subs.vtt"), "WEBVTT\n").unwrap();
        std::fs::write(
            dir.path().join("asset.json"),
            r#"{"tracks":[{"id":"","path":"subs.vtt","type":"text"}]}"#,
        )
        .unwrap();
        let op = Operator::new(Fs::default().root(dir.path().to_str().unwrap())).unwrap();

        let err = Asset::read(&op, "asset.json").await.unwrap_err();

        assert!(matches!(err, CoreError::InvalidDescriptor(_)));
    }

    #[test]
    fn grouping_fields_default_when_absent() {
        let a: Asset = serde_json::from_str(r#"{"tracks": []}"#).unwrap();
        assert_eq!(a.min_segment_length_ms, 0);
        assert!(a.segment_boundaries_ms.is_empty());
    }

    #[test]
    fn grouping_fields_parse_from_the_wire() {
        let a: Asset = serde_json::from_str(
            r#"{"min_segment_length": 3000, "segment_boundaries": [683640], "tracks": []}"#,
        )
        .unwrap();
        assert_eq!(a.min_segment_length_ms, 3000);
        assert_eq!(a.segment_boundaries_ms, vec![683640]);
    }

    #[test]
    fn grouping_fields_serialize_under_their_wire_names() {
        let a = Asset {
            min_segment_length_ms: 3000,
            segment_boundaries_ms: vec![683640],
            ..Asset::new()
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains(r#""min_segment_length":3000"#));
        assert!(json.contains(r#""segment_boundaries":[683640]"#));
    }

    #[test]
    fn default_grouping_stays_off_the_wire() {
        let json = serde_json::to_string(&Asset::new()).unwrap();
        assert!(!json.contains("min_segment_length"));
        assert!(!json.contains("segment_boundaries"));
    }
}
