//! The domain `Asset`: a list of [`Track`]s plus where the descriptor was
//! sourced from. Built from the model in [`crate::model`].

use opendal::Operator;
use relative_path::RelativePath;

use crate::cmaf::{self, Header, Metadata};
use crate::model::{AssetModel, AudioTrackModel, TrackModel, VideoTrackModel};
use crate::CoreError;

/// A dyndo asset: its tracks and where the descriptor was sourced from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Asset {
    /// The asset's tracks, in no particular order.
    pub tracks: Vec<Track>,
    /// Path of the source descriptor (`asset.json`), used to resolve each
    /// track's relative path.
    pub path: String,
}

/// One representation: a parsed CMAF track plus its (sub)segment map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// Resolved storage path of the track's CMAF file (not relative to the
    /// descriptor).
    pub path: String,
    /// Parsed CMAF header: timescale, duration, bandwidth, init segment.
    pub header: Header,
    /// Codec-specific metadata (video or audio).
    pub metadata: Metadata,
    /// The track's (sub)segments, in presentation order.
    pub segments: Vec<Segment>,
}

/// A (sub)segment's location: byte `offset`/`size` plus `duration` in the track
/// timescale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Byte offset of this (sub)segment within the track file.
    pub offset: u64,
    /// Size of this (sub)segment, in bytes.
    pub size: u64,
    /// Duration of this (sub)segment, in the track timescale.
    pub duration: u64,
}

impl Asset {
    /// An empty asset: no tracks, empty source path.
    pub fn new() -> Asset {
        Asset::default()
    }

    /// Append a track.
    pub fn add_track(&mut self, track: Track) {
        self.tracks.push(track);
    }

    /// Build an [`Asset`] from its wire [`AssetModel`], parsing every track's
    /// CMAF header. `path` is the descriptor's own path, used to resolve each
    /// track's relative path.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from reading or parsing a track.
    pub async fn from_model(
        op: &Operator,
        model: AssetModel,
        path: impl Into<String>,
    ) -> Result<Asset, CoreError> {
        let path = path.into();
        let mut asset = Asset::new();
        for track in model.tracks {
            let rel = match track {
                TrackModel::Video(v) => v.path,
                TrackModel::Audio(a) => a.path,
            };
            asset.add_track(Track::from_path(op, &rel, &path).await?);
        }
        asset.path = path;
        Ok(asset)
    }
}

impl Track {
    /// Build a [`Track`] by parsing the CMAF header at `path` (resolved
    /// relative to the descriptor's own `asset_path`) through `op`.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from reading or parsing the track.
    pub async fn from_path(
        op: &Operator,
        path: &str,
        asset_path: &str,
    ) -> Result<Track, CoreError> {
        let key = RelativePath::new(asset_path)
            .parent()
            .expect("descriptor path always has a parent")
            .join(path)
            .normalize()
            .into_string();
        let (header, segments, metadata) = cmaf::header(op, &key).await?;
        Ok(Track {
            path: key,
            header,
            metadata,
            segments,
        })
    }

    /// Read the init segment (`ftyp`+`moov`) bytes through `op`.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    pub async fn init_segment_bytes(&self, op: &Operator) -> Result<Vec<u8>, CoreError> {
        let s = self.header.init_segment;
        cmaf::read(op, &self.path, s.offset, s.size).await
    }

    /// Read the media (sub)segment starting at presentation `time` through `op`,
    /// or `None` if no segment starts exactly there.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    pub async fn segment_bytes(
        &self,
        op: &Operator,
        time: u64,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let mut t = self.header.earliest_presentation_time;
        for seg in &self.segments {
            if t == time {
                return Ok(Some(
                    cmaf::read(op, &self.path, seg.offset, seg.size).await?,
                ));
            }
            t += seg.duration;
        }
        Ok(None)
    }

    /// This track's total presentation duration, in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        units_to_ms(self.header.duration, self.header.timescale)
    }

    /// The longest (sub)segment in this track, in milliseconds (0 if it has none).
    pub fn max_segment_duration_ms(&self) -> u64 {
        self.segments
            .iter()
            .map(|s| units_to_ms(s.duration, self.header.timescale))
            .max()
            .unwrap_or(0)
    }

    /// The representation id, computed from the codec fourcc, dimensions/channels
    /// and bandwidth (e.g. `video_avc1_1080_4807228`, `audio_mp4a_nld_2_196918`).
    pub fn id(&self) -> String {
        match &self.metadata {
            Metadata::Video(v) => format!(
                "video_{}_{}_{}",
                v.codec.fourcc(),
                v.height,
                self.header.bandwidth
            ),
            Metadata::Audio(a) => format!(
                "audio_{}_{}_{}_{}",
                a.codec.fourcc(),
                a.language,
                a.channels,
                self.header.bandwidth
            ),
        }
    }
}

impl Track {
    /// Project to the wire [`TrackModel`], relativizing the stored (resolved)
    /// key back to a path relative to the descriptor `asset_path`.
    fn to_model(&self, asset_path: &str) -> TrackModel {
        let path = RelativePath::new(asset_path)
            .parent()
            .expect("descriptor path always has a parent")
            .relative(self.path.as_str())
            .into_string();
        let id = self.id();
        let timescale = self.header.timescale;
        match &self.metadata {
            Metadata::Video(v) => TrackModel::Video(VideoTrackModel {
                id,
                path,
                fourcc: v.codec.fourcc().to_string(),
                timescale,
                width: v.width,
                height: v.height,
            }),
            Metadata::Audio(a) => TrackModel::Audio(AudioTrackModel {
                id,
                path,
                fourcc: a.codec.fourcc().to_string(),
                timescale,
                sample_rate: a.sample_rate,
                channels: a.channels,
                language: Some(a.language.clone()),
            }),
        }
    }
}

impl From<&Asset> for AssetModel {
    fn from(asset: &Asset) -> AssetModel {
        AssetModel {
            tracks: asset
                .tracks
                .iter()
                .map(|t| t.to_model(&asset.path))
                .collect(),
        }
    }
}

/// Convert a count of `timescale`-units to milliseconds, truncating toward zero.
fn units_to_ms(units: u64, timescale: u32) -> u64 {
    (units as u128 * 1000 / timescale as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{Header, VideoMetadata};
    use crate::codec::VideoCodec;

    fn track(timescale: u32, duration: u64, seg_durations: &[u64]) -> Track {
        Track {
            path: String::new(),
            header: Header {
                timescale,
                duration,
                bandwidth: 0,
                earliest_presentation_time: 0,
                init_segment: Segment {
                    offset: 0,
                    size: 0,
                    duration: 0,
                },
            },
            metadata: Metadata::Video(VideoMetadata {
                codec: VideoCodec::Avc {
                    profile: 0,
                    constraints: 0,
                    level: 0,
                },
                width: 0,
                height: 0,
                frame_rate: (0, 1),
            }),
            segments: seg_durations
                .iter()
                .map(|&d| Segment {
                    offset: 0,
                    size: 0,
                    duration: d,
                })
                .collect(),
        }
    }

    #[test]
    fn duration_ms_scales_units_by_timescale() {
        // 1_800_000 units @ 90_000 = 20 s
        assert_eq!(track(90_000, 1_800_000, &[]).duration_ms(), 20_000);
    }

    #[test]
    fn duration_ms_truncates_toward_zero() {
        // 90_089 units @ 90_000 = 1.000988… s
        assert_eq!(track(90_000, 90_089, &[]).duration_ms(), 1000);
    }

    #[test]
    fn max_segment_duration_ms_is_the_longest_segment() {
        // @48_000: 48_000→1000 ms, 96_000→2000 ms, 24_000→500 ms
        assert_eq!(
            track(48_000, 0, &[48_000, 96_000, 24_000]).max_segment_duration_ms(),
            2000
        );
    }

    #[test]
    fn max_segment_duration_ms_is_zero_without_segments() {
        assert_eq!(track(48_000, 0, &[]).max_segment_duration_ms(), 0);
    }
}
