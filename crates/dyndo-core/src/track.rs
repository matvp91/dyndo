//! The `Track`: one struct for every media type, with the per-type fields
//! split off into [`Metadata`]. Tracks deserialize directly from descriptor
//! (`asset.json`) entries; serialization goes through `track_wire`, which
//! adds derived debug-only fields.

use bytes::Bytes;
use opendal::Operator;
use relative_path::RelativePath;
use serde::Deserialize;

use crate::error::CoreError;
use crate::header::Header;
use crate::header_cmaf::HeaderCmaf;
use crate::metadata::Metadata;
use crate::segment::Segment;
use crate::segment_utils;

/// One of the asset's tracks: the identity and location every media type
/// shares, with the per-type fields split off into `metadata`.
/// `Serialize` is hand-written in `track_wire` to add derived debug fields.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Track {
    /// The descriptor's stored representation id; empty when it declares
    /// none. Consumers key by [`Track::id`], which falls back to a derived
    /// id.
    pub id: String,
    /// Path of the track's file, relative to the asset descriptor
    /// (`asset.json`) that declares it. Reads resolve it against the
    /// descriptor's location on the fly; it is never stored resolved.
    pub path: String,
    /// The track file's parsed header, `None` until probed. Never on the
    /// wire; access through [`Track::header`].
    #[serde(skip)]
    header: Option<Header>,
    /// The track's media type and its per-type fields, tagged on the wire by
    /// `"type": "video"|"audio"|"text"`.
    #[serde(flatten)]
    pub metadata: Metadata,
}

impl Track {
    /// Read the track file at `path`, relative to `asset_descriptor_path`'s
    /// directory, through `op`: its header and the metadata the file
    /// declares. Each read fetches the file for itself; the two run
    /// concurrently. The track keeps the descriptor-relative `path`; the id
    /// starts empty until assigned.
    ///
    /// # Errors
    /// [`CoreError::UnsupportedFormat`] if `path`'s extension maps to no
    /// supported format; otherwise any [`CoreError`] from reading or parsing
    /// the track.
    pub async fn read(
        op: &Operator,
        asset_descriptor_path: &str,
        path: &str,
    ) -> Result<Track, CoreError> {
        let resolved = resolve(asset_descriptor_path, path);
        let (header, metadata) =
            tokio::try_join!(Header::read(op, &resolved), Metadata::read(op, &resolved))?;
        Ok(Track {
            id: String::new(),
            path: path.to_string(),
            header: Some(header),
            metadata,
        })
    }

    /// Read the header of the track file at the track's `path`, relative to
    /// `asset_descriptor_path`'s directory, through `op` and store it, e.g.
    /// after deserializing the track from a descriptor.
    ///
    /// # Errors
    /// [`CoreError::UnsupportedFormat`] if the path's extension maps to no
    /// supported format; otherwise any [`CoreError`] from reading or
    /// parsing the file.
    pub async fn read_header(
        &mut self,
        op: &Operator,
        asset_descriptor_path: &str,
    ) -> Result<(), CoreError> {
        let resolved = resolve(asset_descriptor_path, &self.path);
        self.header = Some(Header::read(op, &resolved).await?);
        Ok(())
    }

    /// The track file's parsed header.
    ///
    /// # Panics
    /// If the track has not been probed (`header` is `None`).
    pub fn header(&self) -> &Header {
        self.header.as_ref().expect("track not probed")
    }

    /// The track's CMAF header.
    ///
    /// # Panics
    /// If the track has not been probed, or is raw.
    pub fn cmaf(&self) -> &HeaderCmaf {
        let Header::Cmaf(h) = self.header() else {
            panic!("track is not CMAF");
        };
        h
    }

    /// The representation id: the descriptor's stored id when present, else
    /// derived from the track's distinguishing fields —
    /// `video_{height}_{bandwidth}`, `audio_{language}_{channels}_{bandwidth}`,
    /// or `text_{language}`. Manifests and segment routes both key
    /// representations by this value.
    ///
    /// # Panics
    /// If the track has not been probed: the video and audio derivations
    /// read the header.
    pub fn id(&self) -> String {
        if !self.id.is_empty() {
            return self.id.clone();
        }
        match &self.metadata {
            Metadata::Video(v) => format!("video_{}_{}", v.height, self.cmaf().bandwidth()),
            Metadata::Audio(a) => format!(
                "audio_{}_{}_{}",
                a.language,
                a.channels,
                self.cmaf().bandwidth()
            ),
            Metadata::Text(t) => format!("text_{}", t.language),
        }
    }

    /// The MIME type of the track's file: the CMAF container type for its
    /// media type (`video/mp4`, `audio/mp4`, or `application/mp4`), or
    /// `text/vtt` for a raw VTT file.
    ///
    /// # Panics
    /// If the track has not been probed.
    pub fn mime_type(&self) -> &'static str {
        match (self.header(), &self.metadata) {
            (Header::Raw(_), _) => "text/vtt",
            (Header::Cmaf(_), Metadata::Video(_)) => "video/mp4",
            (Header::Cmaf(_), Metadata::Audio(_)) => "audio/mp4",
            (Header::Cmaf(_), Metadata::Text(_)) => "application/mp4",
        }
    }

    /// The track's RFC 6381 codecs parameter (e.g. `"avc1.640028"`), or
    /// `None` for a raw file: a plain `.vtt` declares no codec.
    ///
    /// # Panics
    /// If the track has not been probed.
    pub fn codec(&self) -> Option<&str> {
        match self.header() {
            Header::Cmaf(h) => Some(&h.codec),
            Header::Raw(_) => None,
        }
    }

    /// The track's served (sub)segments, in presentation order: the header's
    /// raw CMAF fragments grouped to at least `min_length_ms`, never across
    /// a splice point in `boundaries_ms`. A `min_length_ms` of 0 serves each
    /// fragment as its own segment. Both values come from the asset
    /// descriptor; manifest builders and the segment route must pass the
    /// same pair or advertised segment times will not resolve.
    ///
    /// # Panics
    /// If the track has not been probed, or is raw: a raw file has no
    /// segment map of its own — its segmentation follows the asset's other
    /// tracks.
    pub fn segments(&self, boundaries_ms: &[u64], min_length_ms: u64) -> Vec<Segment> {
        let h = self.cmaf();
        segment_utils::group_segments(&h.segments, h.timescale, boundaries_ms, min_length_ms)
    }

    /// Read the bytes of the track's init segment (`ftyp`+`moov`) through
    /// `op`, resolving the track's `path` against `asset_descriptor_path`'s
    /// directory.
    ///
    /// # Errors
    /// [`CoreError::Storage`] if the ranged read fails.
    ///
    /// # Panics
    /// If the track has not been probed, or is raw.
    pub async fn read_init_segment(
        &self,
        op: &Operator,
        asset_descriptor_path: &str,
    ) -> Result<Bytes, CoreError> {
        let resolved = resolve(asset_descriptor_path, &self.path);
        let range = self.cmaf().init_segment().range();
        Ok(op.read_with(&resolved).range(range).await?.to_bytes())
    }

    /// Read the bytes of the served (sub)segment starting at presentation
    /// `time` (in the track timescale) through `op`, resolving the track's
    /// `path` against `asset_descriptor_path`'s directory. `time` is matched
    /// against the served segments — pass the same grouping pair the
    /// manifest was built with. `None` when no (sub)segment starts at
    /// `time`.
    ///
    /// # Errors
    /// [`CoreError::Storage`] if the ranged read fails.
    ///
    /// # Panics
    /// If the track has not been probed, or is raw.
    pub async fn read_segment(
        &self,
        op: &Operator,
        asset_descriptor_path: &str,
        time: u64,
        boundaries_ms: &[u64],
        min_length_ms: u64,
    ) -> Result<Option<Bytes>, CoreError> {
        let mut start = self.cmaf().earliest_presentation_time;
        for seg in self.segments(boundaries_ms, min_length_ms) {
            if start == time {
                let resolved = resolve(asset_descriptor_path, &self.path);
                let buf = op.read_with(&resolved).range(seg.range()).await?;
                return Ok(Some(buf.to_bytes()));
            }
            start += seg.duration;
        }
        Ok(None)
    }

    /// This track's total presentation duration, in milliseconds. `0` for
    /// raw tracks: a raw file declares no duration of its own.
    ///
    /// # Panics
    /// If the track has not been probed.
    pub fn duration_ms(&self) -> u64 {
        let Header::Cmaf(cmaf) = self.header() else {
            return 0;
        };
        units_to_ms(cmaf.duration(), cmaf.timescale)
    }

    /// The longest served (sub)segment in this track, in milliseconds,
    /// under the same grouping pair as [`Track::segments`]. `0` if it has
    /// none, or for raw tracks: a raw file has no segment map of its own.
    ///
    /// # Panics
    /// If the track has not been probed.
    pub fn max_segment_duration_ms(&self, boundaries_ms: &[u64], min_length_ms: u64) -> u64 {
        let Header::Cmaf(cmaf) = self.header() else {
            return 0;
        };
        self.segments(boundaries_ms, min_length_ms)
            .iter()
            .map(|s| units_to_ms(s.duration, cmaf.timescale))
            .max()
            .unwrap_or(0)
    }
}

/// Convert a count of `timescale`-units to milliseconds, truncating toward
/// zero.
fn units_to_ms(units: u64, timescale: u32) -> u64 {
    (units as u128 * 1000 / timescale as u128) as u64
}

/// Resolve `path`, given relative to `asset_descriptor_path`'s directory,
/// into a normalized storage path.
fn resolve(asset_descriptor_path: &str, path: &str) -> String {
    RelativePath::new(asset_descriptor_path)
        .parent()
        .expect("descriptor path always has a parent")
        .join(path)
        .normalize()
        .into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_joins_a_sibling_against_the_descriptor_dir() {
        assert_eq!(resolve("out/asset.json", "video.mp4"), "out/video.mp4");
    }

    #[test]
    fn resolve_normalizes_parent_segments() {
        assert_eq!(resolve("out/asset.json", "../video.mp4"), "video.mp4");
    }

    #[test]
    fn resolve_from_a_root_descriptor_is_the_path_itself() {
        assert_eq!(resolve("asset.json", "subs/eng.vtt"), "subs/eng.vtt");
    }
}
