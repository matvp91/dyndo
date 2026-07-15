//! The domain `Asset`: video and audio tracks plus where the descriptor was
//! sourced from. Built from the model in [`crate::model`].

use opendal::Operator;

use crate::cmaf::{
    self, AudioCmafMetadata, CmafHeader, Metadata, TextCmafMetadata, VideoCmafMetadata,
};
use crate::model::{AssetModel, AudioTrackModel, TextTrackModel, TrackModel, VideoTrackModel};
use crate::utils::path;
use crate::CoreError;

/// A dyndo asset: its tracks and where the descriptor was sourced from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Asset {
    /// The asset's video tracks, in no particular order.
    pub video_tracks: Vec<VideoTrack>,
    /// The asset's audio tracks, in no particular order.
    pub audio_tracks: Vec<AudioTrack>,
    /// The asset's text tracks, in no particular order.
    pub text_tracks: Vec<TextTrack>,
    /// Path of the source descriptor (`asset.json`), used to resolve each
    /// track's relative path.
    pub path: String,
}

/// A (sub)segment's location: byte `offset`/`size` plus `duration` in the track
/// timescale and `duration_ms` in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Byte offset of this (sub)segment within the track file.
    pub offset: u64,
    /// Size of this (sub)segment, in bytes.
    pub size: u64,
    /// Duration of this (sub)segment, in the track timescale.
    pub duration: u64,
    /// Duration of this (sub)segment, in milliseconds. Computed once at probe
    /// from `duration`/timescale using drift-free cumulative boundaries, so a
    /// track's per-segment `duration_ms` values sum to its total ms duration.
    pub duration_ms: u64,
}

/// A parsed CMAF track: its resolved storage path, the [`CmafHeader`] shared by
/// every media type, and the media-specific `cmaf_metadata` that tells the kinds
/// apart. The concrete kinds are the aliases [`VideoTrack`], [`AudioTrack`], and
/// [`TextTrack`]; [`AnyTrack`] is the kind resolved only at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track<M> {
    /// Resolved storage path of the track's CMAF file (not relative to the
    /// descriptor).
    pub path: String,
    /// Parsed CMAF header: timing, init segment, and the (sub)segment map.
    pub cmaf_header: CmafHeader,
    /// Parsed media-specific metadata: codec plus the fields unique to this
    /// media type.
    pub cmaf_metadata: M,
}

/// A parsed CMAF video track: codec, dimensions, and frame rate.
pub type VideoTrack = Track<VideoCmafMetadata>;
/// A parsed CMAF audio track: codec, sample rate, channels, and language.
pub type AudioTrack = Track<AudioCmafMetadata>;
/// A parsed CMAF timed-text track: codec and language.
pub type TextTrack = Track<TextCmafMetadata>;
/// A parsed CMAF track whose media type is known only at runtime (e.g. resolved
/// from a descriptor id).
pub type AnyTrack = Track<Metadata>;

/// The media-type-specific behaviour a [`Track<M>`] delegates to: how the
/// metadata is recovered from its wire model and a freshly-probed CMAF header,
/// its representation id, and its segments' MIME type. Implemented by each
/// concrete metadata type and by the runtime-typed [`Metadata`] enum, so a
/// single generic [`Track<M>`] serves both statically- and dynamically-typed
/// tracks.
pub trait TrackMetadata: Sized {
    /// The wire model this metadata is built from ([`VideoTrackModel`],
    /// [`AudioTrackModel`], [`TextTrackModel`], or the [`TrackModel`] enum for
    /// [`AnyTrack`]).
    type Model;

    /// The `model`'s source path, relative to the descriptor.
    fn model_path(model: &Self::Model) -> &str;

    /// Recover this metadata from the `probed` CMAF metadata declared by
    /// `model`, or `None` when the file's media type contradicts the model
    /// (the descriptor and its file have drifted apart).
    fn from_probe(model: &Self::Model, probed: Metadata) -> Option<Self>;

    /// The representation id, computed from the codec fourcc, dimensions/channels
    /// and the `header`'s bandwidth (e.g. `video_avc1_1080_4807228`,
    /// `audio_mp4a_nld_2_196918`).
    fn id(&self, header: &CmafHeader) -> String;

    /// The `video/mp4` / `audio/mp4` / `application/mp4` MIME type of the
    /// track's CMAF segments.
    fn mime_type(&self) -> &'static str;
}

impl<M> Track<M> {
    /// Resolved storage path of the track's CMAF file.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The track's parsed CMAF header (timing, init segment, and segment map).
    pub fn cmaf_header(&self) -> &CmafHeader {
        &self.cmaf_header
    }

    /// Read the init segment (`ftyp`+`moov`) bytes through `op`.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    pub async fn init_segment_bytes(&self, op: &Operator) -> Result<Vec<u8>, CoreError> {
        let s = self.cmaf_header().init_segment;
        cmaf::read_range(op, self.path(), s.offset, s.size).await
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
        let mut t = self.cmaf_header().earliest_presentation_time;
        for seg in &self.cmaf_header().segments {
            if t == time {
                return Ok(Some(
                    cmaf::read_range(op, self.path(), seg.offset, seg.size).await?,
                ));
            }
            t += seg.duration;
        }
        Ok(None)
    }

    /// This track's total presentation duration, in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        units_to_ms(self.cmaf_header().duration, self.cmaf_header().timescale)
    }

    /// The longest (sub)segment in this track, in milliseconds (0 if it has none).
    pub fn max_segment_duration_ms(&self) -> u64 {
        self.cmaf_header()
            .segments
            .iter()
            .map(|s| s.duration_ms)
            .max()
            .unwrap_or(0)
    }
}

impl<M: TrackMetadata> Track<M> {
    /// Build the track by parsing the CMAF header at `model`'s path (resolved
    /// against the descriptor's own `descriptor_path`) through `op`.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from reading or parsing the track, or a
    /// [`CoreError::Container`] if the file's media type contradicts the model
    /// (e.g. a video model whose CMAF parses as audio), which means the
    /// descriptor and its file have drifted apart.
    pub async fn from_model(
        op: &Operator,
        model: &M::Model,
        descriptor_path: &str,
    ) -> Result<Track<M>, CoreError> {
        let path = path::resolve(descriptor_path, M::model_path(model));
        let (cmaf_header, cmaf_metadata) = cmaf::probe(op, &path).await?;
        let Some(cmaf_metadata) = M::from_probe(model, cmaf_metadata) else {
            return Err(CoreError::Container(format!(
                "track at {path}: descriptor type and CMAF type disagree"
            )));
        };
        Ok(Track {
            path,
            cmaf_header,
            cmaf_metadata,
        })
    }

    /// The representation id (see [`TrackMetadata::id`]).
    pub fn id(&self) -> String {
        self.cmaf_metadata.id(&self.cmaf_header)
    }

    /// The MIME type of this track's CMAF segments (see
    /// [`TrackMetadata::mime_type`]).
    pub fn mime_type(&self) -> &'static str {
        self.cmaf_metadata.mime_type()
    }
}

impl TrackMetadata for VideoCmafMetadata {
    type Model = VideoTrackModel;

    fn model_path(model: &VideoTrackModel) -> &str {
        &model.path
    }

    fn from_probe(_model: &VideoTrackModel, probed: Metadata) -> Option<VideoCmafMetadata> {
        match probed {
            Metadata::Video(m) => Some(m),
            _ => None,
        }
    }

    fn id(&self, header: &CmafHeader) -> String {
        format!(
            "video_{}_{}_{}",
            self.codec.fourcc(),
            self.height,
            header.bandwidth
        )
    }

    fn mime_type(&self) -> &'static str {
        "video/mp4"
    }
}

impl TrackMetadata for AudioCmafMetadata {
    type Model = AudioTrackModel;

    fn model_path(model: &AudioTrackModel) -> &str {
        &model.path
    }

    fn from_probe(_model: &AudioTrackModel, probed: Metadata) -> Option<AudioCmafMetadata> {
        match probed {
            Metadata::Audio(m) => Some(m),
            _ => None,
        }
    }

    fn id(&self, header: &CmafHeader) -> String {
        format!(
            "audio_{}_{}_{}_{}",
            self.codec.fourcc(),
            self.language,
            self.channels,
            header.bandwidth
        )
    }

    fn mime_type(&self) -> &'static str {
        "audio/mp4"
    }
}

impl TrackMetadata for TextCmafMetadata {
    type Model = TextTrackModel;

    fn model_path(model: &TextTrackModel) -> &str {
        &model.path
    }

    fn from_probe(_model: &TextTrackModel, probed: Metadata) -> Option<TextCmafMetadata> {
        match probed {
            Metadata::Text(m) => Some(m),
            _ => None,
        }
    }

    fn id(&self, _header: &CmafHeader) -> String {
        format!("text_{}_{}", self.codec.fourcc(), self.language)
    }

    fn mime_type(&self) -> &'static str {
        "application/mp4"
    }
}

impl TrackMetadata for Metadata {
    type Model = TrackModel;

    fn model_path(model: &TrackModel) -> &str {
        model.path()
    }

    fn from_probe(model: &TrackModel, probed: Metadata) -> Option<Metadata> {
        // AnyTrack keeps whatever the file actually is, but only when its media
        // type matches the one the descriptor declared.
        let agree = matches!(
            (model, &probed),
            (TrackModel::Video(_), Metadata::Video(_))
                | (TrackModel::Audio(_), Metadata::Audio(_))
                | (TrackModel::Text(_), Metadata::Text(_))
        );
        agree.then_some(probed)
    }

    fn id(&self, header: &CmafHeader) -> String {
        match self {
            Metadata::Video(m) => m.id(header),
            Metadata::Audio(m) => m.id(header),
            Metadata::Text(m) => m.id(header),
        }
    }

    fn mime_type(&self) -> &'static str {
        match self {
            Metadata::Video(m) => m.mime_type(),
            Metadata::Audio(m) => m.mime_type(),
            Metadata::Text(m) => m.mime_type(),
        }
    }
}

impl Asset {
    /// An empty asset: no tracks, empty source path.
    pub fn new() -> Asset {
        Asset::default()
    }

    /// Parse the CMAF file at `file_path` and add it as the video, audio, or
    /// text track its `hdlr` box declares. `descriptor_path` resolves `file_path`
    /// relative to the descriptor. Tracks carry no ordering guarantee.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from reading or parsing the track.
    pub async fn add_track(
        &mut self,
        op: &Operator,
        file_path: &str,
        descriptor_path: &str,
    ) -> Result<(), CoreError> {
        let path = path::resolve(descriptor_path, file_path);
        let (cmaf_header, cmaf_metadata) = cmaf::probe(op, &path).await?;
        match cmaf_metadata {
            Metadata::Video(cmaf_metadata) => self.video_tracks.push(VideoTrack {
                path,
                cmaf_header,
                cmaf_metadata,
            }),
            Metadata::Audio(cmaf_metadata) => self.audio_tracks.push(AudioTrack {
                path,
                cmaf_header,
                cmaf_metadata,
            }),
            Metadata::Text(cmaf_metadata) => self.text_tracks.push(TextTrack {
                path,
                cmaf_header,
                cmaf_metadata,
            }),
        }
        Ok(())
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
        for track in &model.tracks {
            match track {
                TrackModel::Video(v) => {
                    asset
                        .video_tracks
                        .push(VideoTrack::from_model(op, v, &path).await?);
                }
                TrackModel::Audio(a) => {
                    asset
                        .audio_tracks
                        .push(AudioTrack::from_model(op, a, &path).await?);
                }
                TrackModel::Text(t) => {
                    asset
                        .text_tracks
                        .push(TextTrack::from_model(op, t, &path).await?);
                }
            }
        }
        asset.path = path;
        Ok(asset)
    }
}

impl VideoTrack {
    /// Project to the wire [`TrackModel`], relativizing the stored (resolved)
    /// path back to a file path relative to the descriptor `descriptor_path`.
    fn to_model(&self, descriptor_path: &str) -> TrackModel {
        TrackModel::Video(VideoTrackModel {
            id: self.id(),
            path: path::relativize(descriptor_path, &self.path),
            fourcc: self.cmaf_metadata.codec.fourcc().to_string(),
            timescale: self.cmaf_header.timescale,
            width: self.cmaf_metadata.width,
            height: self.cmaf_metadata.height,
        })
    }
}

impl AudioTrack {
    /// Project to the wire [`TrackModel`], relativizing the stored (resolved)
    /// path back to a file path relative to the descriptor `descriptor_path`.
    fn to_model(&self, descriptor_path: &str) -> TrackModel {
        TrackModel::Audio(AudioTrackModel {
            id: self.id(),
            path: path::relativize(descriptor_path, &self.path),
            fourcc: self.cmaf_metadata.codec.fourcc().to_string(),
            timescale: self.cmaf_header.timescale,
            sample_rate: self.cmaf_metadata.sample_rate,
            channels: self.cmaf_metadata.channels,
            language: Some(self.cmaf_metadata.language.clone()),
        })
    }
}

impl TextTrack {
    /// Project to the wire [`TrackModel`], relativizing the stored (resolved)
    /// path back to a file path relative to the descriptor `descriptor_path`.
    fn to_model(&self, descriptor_path: &str) -> TrackModel {
        TrackModel::Text(TextTrackModel {
            id: self.id(),
            path: path::relativize(descriptor_path, &self.path),
            fourcc: self.cmaf_metadata.codec.fourcc().to_string(),
            timescale: self.cmaf_header.timescale,
            language: self.cmaf_metadata.language.clone(),
        })
    }
}

impl From<&Asset> for AssetModel {
    fn from(asset: &Asset) -> AssetModel {
        let video = asset.video_tracks.iter().map(|t| t.to_model(&asset.path));
        let audio = asset.audio_tracks.iter().map(|t| t.to_model(&asset.path));
        let text = asset.text_tracks.iter().map(|t| t.to_model(&asset.path));
        AssetModel {
            tracks: video.chain(audio).chain(text).collect(),
        }
    }
}

/// Convert a count of `timescale`-units to milliseconds, truncating toward zero.
pub(crate) fn units_to_ms(units: u64, timescale: u32) -> u64 {
    (units as u128 * 1000 / timescale as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{TextCodec, VideoCodec};

    fn text_track(language: &str) -> TextTrack {
        TextTrack {
            path: String::new(),
            cmaf_header: CmafHeader {
                timescale: 1000,
                duration: 0,
                bandwidth: 0,
                earliest_presentation_time: 0,
                init_segment: Segment {
                    offset: 0,
                    size: 0,
                    duration: 0,
                    duration_ms: 0,
                },
                segments: Vec::new(),
            },
            cmaf_metadata: TextCmafMetadata {
                codec: TextCodec::Wvtt,
                language: language.to_string(),
            },
        }
    }

    #[test]
    fn text_track_id_is_text_fourcc_language() {
        assert_eq!(text_track("und").id(), "text_wvtt_und");
    }

    fn video_track(timescale: u32, duration: u64, seg_durations: &[u64]) -> VideoTrack {
        VideoTrack {
            path: String::new(),
            cmaf_header: CmafHeader {
                timescale,
                duration,
                bandwidth: 0,
                earliest_presentation_time: 0,
                init_segment: Segment {
                    offset: 0,
                    size: 0,
                    duration: 0,
                    duration_ms: 0,
                },
                segments: seg_durations
                    .iter()
                    .map(|&d| Segment {
                        offset: 0,
                        size: 0,
                        duration: d,
                        duration_ms: (d as u128 * 1000 / timescale as u128) as u64,
                    })
                    .collect(),
            },
            cmaf_metadata: VideoCmafMetadata {
                codec: VideoCodec::Avc {
                    profile: 0,
                    constraints: 0,
                    level: 0,
                },
                width: 0,
                height: 0,
                frame_rate: (0, 1),
            },
        }
    }

    #[test]
    fn duration_ms_scales_units_by_timescale() {
        // 1_800_000 units @ 90_000 = 20 s
        assert_eq!(video_track(90_000, 1_800_000, &[]).duration_ms(), 20_000);
    }

    #[test]
    fn duration_ms_truncates_toward_zero() {
        // 90_089 units @ 90_000 = 1.000988… s
        assert_eq!(video_track(90_000, 90_089, &[]).duration_ms(), 1000);
    }

    #[test]
    fn max_segment_duration_ms_is_the_longest_segment() {
        // @48_000: 48_000→1000 ms, 96_000→2000 ms, 24_000→500 ms
        assert_eq!(
            video_track(48_000, 0, &[48_000, 96_000, 24_000]).max_segment_duration_ms(),
            2000
        );
    }

    #[test]
    fn max_segment_duration_ms_is_zero_without_segments() {
        assert_eq!(video_track(48_000, 0, &[]).max_segment_duration_ms(), 0);
    }
}
