//! The domain `Asset`: video and audio tracks plus where the descriptor was
//! sourced from. Built from the model in [`crate::model`].

use opendal::Operator;
use relative_path::RelativePath;

use crate::cmaf::{
    self, AudioCmafMetadata, CmafHeader, Metadata, TextCmafMetadata, VideoCmafMetadata,
};
use crate::model::{AssetModel, AudioTrackModel, TextTrackModel, TrackModel, VideoTrackModel};
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

/// A parsed CMAF video track: its [`CmafHeader`] and [`VideoCmafMetadata`],
/// plus the resolved storage path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTrack {
    /// Resolved storage path of the track's CMAF file (not relative to the
    /// descriptor).
    pub path: String,
    /// Parsed CMAF header: timing, init segment, and the (sub)segment map.
    pub cmaf_header: CmafHeader,
    /// Parsed video-specific metadata: codec, dimensions, frame rate.
    pub cmaf_metadata: VideoCmafMetadata,
}

/// A parsed CMAF audio track: its [`CmafHeader`] and [`AudioCmafMetadata`],
/// plus the resolved storage path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTrack {
    /// Resolved storage path of the track's CMAF file (not relative to the
    /// descriptor).
    pub path: String,
    /// Parsed CMAF header: timing, init segment, and the (sub)segment map.
    pub cmaf_header: CmafHeader,
    /// Parsed audio-specific metadata: codec, sample rate, channels, language.
    pub cmaf_metadata: AudioCmafMetadata,
}

/// A parsed CMAF timed-text track: its [`CmafHeader`] and [`TextCmafMetadata`],
/// plus the resolved storage path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTrack {
    /// Resolved storage path of the track's CMAF file (not relative to the
    /// descriptor).
    pub path: String,
    /// Parsed CMAF header: timing, init segment, and the (sub)segment map.
    pub cmaf_header: CmafHeader,
    /// Parsed text-specific metadata: codec and language.
    pub cmaf_metadata: TextCmafMetadata,
}

/// A track whose media type is known only at runtime (e.g. resolved from a
/// descriptor id). Delegates the shared [`Track`] behaviour to the inner
/// concrete track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnyTrack {
    /// A video track.
    Video(VideoTrack),
    /// An audio track.
    Audio(AudioTrack),
    /// A text track.
    Text(TextTrack),
}

/// Behaviour shared by every track kind: how it is built from its wire model,
/// its representation id, and the media-agnostic reads and derivations over its
/// header and (sub)segments.
#[expect(
    async_fn_in_trait,
    reason = "Track is only ever used via static dispatch (concrete types and \
              `T: Track` generics), never `dyn Track`, and the concrete futures \
              are `Send` for callers like the server that need it."
)]
pub trait Track: Sized {
    /// The wire model ([`VideoTrackModel`] or [`AudioTrackModel`]) this track is
    /// built from.
    type Model;

    /// Build the track by parsing the CMAF header at the model's path (resolved
    /// relative to the descriptor's own `descriptor_path`) through `op`.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from reading or parsing the track, or a
    /// [`CoreError::Container`] if the file's media type contradicts the model
    /// (e.g. a video model whose CMAF parses as audio), which means the
    /// descriptor and its file have drifted apart.
    async fn from_model(
        op: &Operator,
        model: &Self::Model,
        descriptor_path: &str,
    ) -> Result<Self, CoreError>;

    /// The representation id, computed from the codec fourcc, dimensions/channels
    /// and bandwidth (e.g. `video_avc1_1080_4807228`, `audio_mp4a_nld_2_196918`).
    fn id(&self) -> String;

    /// The `video/mp4` / `audio/mp4` MIME type of this track's CMAF segments.
    fn mime_type(&self) -> &'static str;

    /// Resolved storage path of the track's CMAF file.
    fn path(&self) -> &str;

    /// The track's parsed CMAF header (timing, init segment, and segment map).
    fn cmaf_header(&self) -> &CmafHeader;

    /// Read the init segment (`ftyp`+`moov`) bytes through `op`.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    async fn init_segment_bytes(&self, op: &Operator) -> Result<Vec<u8>, CoreError> {
        let s = self.cmaf_header().init_segment;
        cmaf::read_range(op, self.path(), s.offset, s.size).await
    }

    /// Read the media (sub)segment starting at presentation `time` through `op`,
    /// or `None` if no segment starts exactly there.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    async fn segment_bytes(&self, op: &Operator, time: u64) -> Result<Option<Vec<u8>>, CoreError> {
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
    fn duration_ms(&self) -> u64 {
        units_to_ms(self.cmaf_header().duration, self.cmaf_header().timescale)
    }

    /// The longest (sub)segment in this track, in milliseconds (0 if it has none).
    fn max_segment_duration_ms(&self) -> u64 {
        self.cmaf_header()
            .segments
            .iter()
            .map(|s| units_to_ms(s.duration, self.cmaf_header().timescale))
            .max()
            .unwrap_or(0)
    }
}

impl Track for VideoTrack {
    type Model = VideoTrackModel;

    async fn from_model(
        op: &Operator,
        model: &VideoTrackModel,
        descriptor_path: &str,
    ) -> Result<VideoTrack, CoreError> {
        let path = resolve(descriptor_path, &model.path);
        let (cmaf_header, cmaf_metadata) = cmaf::probe(op, &path).await?;
        let Metadata::Video(cmaf_metadata) = cmaf_metadata else {
            return Err(CoreError::Container(format!(
                "track at {path}: descriptor type and CMAF type disagree"
            )));
        };
        Ok(VideoTrack {
            path,
            cmaf_header,
            cmaf_metadata,
        })
    }

    fn id(&self) -> String {
        format!(
            "video_{}_{}_{}",
            self.cmaf_metadata.codec.fourcc(),
            self.cmaf_metadata.height,
            self.cmaf_header.bandwidth
        )
    }

    fn mime_type(&self) -> &'static str {
        "video/mp4"
    }

    fn path(&self) -> &str {
        &self.path
    }

    fn cmaf_header(&self) -> &CmafHeader {
        &self.cmaf_header
    }
}

impl Track for AudioTrack {
    type Model = AudioTrackModel;

    async fn from_model(
        op: &Operator,
        model: &AudioTrackModel,
        descriptor_path: &str,
    ) -> Result<AudioTrack, CoreError> {
        let path = resolve(descriptor_path, &model.path);
        let (cmaf_header, cmaf_metadata) = cmaf::probe(op, &path).await?;
        let Metadata::Audio(cmaf_metadata) = cmaf_metadata else {
            return Err(CoreError::Container(format!(
                "track at {path}: descriptor type and CMAF type disagree"
            )));
        };
        Ok(AudioTrack {
            path,
            cmaf_header,
            cmaf_metadata,
        })
    }

    fn id(&self) -> String {
        format!(
            "audio_{}_{}_{}_{}",
            self.cmaf_metadata.codec.fourcc(),
            self.cmaf_metadata.language,
            self.cmaf_metadata.channels,
            self.cmaf_header.bandwidth
        )
    }

    fn mime_type(&self) -> &'static str {
        "audio/mp4"
    }

    fn path(&self) -> &str {
        &self.path
    }

    fn cmaf_header(&self) -> &CmafHeader {
        &self.cmaf_header
    }
}

impl Track for TextTrack {
    type Model = TextTrackModel;

    async fn from_model(
        op: &Operator,
        model: &TextTrackModel,
        descriptor_path: &str,
    ) -> Result<TextTrack, CoreError> {
        let path = resolve(descriptor_path, &model.path);
        let (cmaf_header, cmaf_metadata) = cmaf::probe(op, &path).await?;
        let Metadata::Text(cmaf_metadata) = cmaf_metadata else {
            return Err(CoreError::Container(format!(
                "track at {path}: descriptor type and CMAF type disagree"
            )));
        };
        Ok(TextTrack {
            path,
            cmaf_header,
            cmaf_metadata,
        })
    }

    fn id(&self) -> String {
        format!(
            "text_{}_{}",
            self.cmaf_metadata.codec.fourcc(),
            self.cmaf_metadata.language
        )
    }

    fn mime_type(&self) -> &'static str {
        "application/mp4"
    }

    fn path(&self) -> &str {
        &self.path
    }

    fn cmaf_header(&self) -> &CmafHeader {
        &self.cmaf_header
    }
}

impl Track for AnyTrack {
    type Model = TrackModel;

    async fn from_model(
        op: &Operator,
        model: &TrackModel,
        descriptor_path: &str,
    ) -> Result<AnyTrack, CoreError> {
        Ok(match model {
            TrackModel::Video(v) => {
                AnyTrack::Video(VideoTrack::from_model(op, v, descriptor_path).await?)
            }
            TrackModel::Audio(a) => {
                AnyTrack::Audio(AudioTrack::from_model(op, a, descriptor_path).await?)
            }
            TrackModel::Text(t) => {
                AnyTrack::Text(TextTrack::from_model(op, t, descriptor_path).await?)
            }
        })
    }

    fn id(&self) -> String {
        match self {
            AnyTrack::Video(t) => t.id(),
            AnyTrack::Audio(t) => t.id(),
            AnyTrack::Text(t) => t.id(),
        }
    }

    fn mime_type(&self) -> &'static str {
        match self {
            AnyTrack::Video(t) => t.mime_type(),
            AnyTrack::Audio(t) => t.mime_type(),
            AnyTrack::Text(t) => t.mime_type(),
        }
    }

    fn path(&self) -> &str {
        match self {
            AnyTrack::Video(t) => t.path(),
            AnyTrack::Audio(t) => t.path(),
            AnyTrack::Text(t) => t.path(),
        }
    }

    fn cmaf_header(&self) -> &CmafHeader {
        match self {
            AnyTrack::Video(t) => t.cmaf_header(),
            AnyTrack::Audio(t) => t.cmaf_header(),
            AnyTrack::Text(t) => t.cmaf_header(),
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
        let path = resolve(descriptor_path, file_path);
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
            path: relativize(descriptor_path, &self.path),
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
            path: relativize(descriptor_path, &self.path),
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
            path: relativize(descriptor_path, &self.path),
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

/// Resolve a descriptor-relative `file_path` against the descriptor's own
/// `descriptor_path`, yielding the track file's resolved storage path.
fn resolve(descriptor_path: &str, file_path: &str) -> String {
    RelativePath::new(descriptor_path)
        .parent()
        .expect("descriptor path always has a parent")
        .join(file_path)
        .normalize()
        .into_string()
}

/// Relativize a resolved storage `path` back to a `file_path` relative to the
/// descriptor's own `descriptor_path`.
fn relativize(descriptor_path: &str, path: &str) -> String {
    RelativePath::new(descriptor_path)
        .parent()
        .expect("descriptor path always has a parent")
        .relative(path)
        .into_string()
}

/// Convert a count of `timescale`-units to milliseconds, truncating toward zero.
fn units_to_ms(units: u64, timescale: u32) -> u64 {
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
                },
                segments: seg_durations
                    .iter()
                    .map(|&d| Segment {
                        offset: 0,
                        size: 0,
                        duration: d,
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
