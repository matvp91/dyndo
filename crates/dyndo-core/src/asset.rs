//! The domain `Asset`: video, audio, and text tracks plus where the descriptor
//! was sourced from. Built from the model in [`crate::model`].

use bytes::Bytes;
use futures_util::future::try_join_all;
use opendal::Operator;

use crate::CoreError;
use crate::cmaf::{
    self, AudioCmafMetadata, CmafHeader, CmafMetadata, TextCmafMetadata, VideoCmafMetadata,
};
use crate::codec::{AudioCodec, TextCodec, VideoCodec};
use crate::model::{AssetModel, AudioTrackModel, TextTrackModel, TrackModel, VideoTrackModel};
use crate::path;

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

/// Serve-time segmentation policy from the descriptor: how a track's raw CMAF
/// fragments are grouped into served segments. `Default` (no minimum, no
/// boundaries) leaves every fragment as its own segment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Segmentation {
    /// Minimum length of a served segment, in milliseconds. Whole fragments
    /// are accumulated until a group reaches this; `None` (or 0) disables
    /// grouping. The last group before a splice point or the track end may be
    /// shorter.
    pub min_segment_length_ms: Option<u64>,
    /// Splice points, in milliseconds from the start of the presentation.
    /// A served segment never spans one (ad-insertion cut points).
    pub segment_boundaries_ms: Vec<u64>,
}

impl Segmentation {
    /// Build a policy, validating that `segment_boundaries_ms` is strictly
    /// ascending and non-zero.
    ///
    /// # Errors
    /// [`CoreError::InvalidDescriptor`] when the boundaries are unsorted,
    /// duplicated, or contain 0 — a broken descriptor, not something to
    /// silently repair.
    pub fn new(
        min_segment_length_ms: Option<u64>,
        segment_boundaries_ms: Vec<u64>,
    ) -> Result<Segmentation, CoreError> {
        let ascending = segment_boundaries_ms.windows(2).all(|w| w[0] < w[1]);
        if !ascending || segment_boundaries_ms.first() == Some(&0) {
            return Err(CoreError::InvalidDescriptor(
                "segment_boundaries must be strictly ascending and non-zero".to_string(),
            ));
        }
        Ok(Segmentation {
            min_segment_length_ms,
            segment_boundaries_ms,
        })
    }
}

mod sealed {
    use crate::cmaf::CmafHeader;

    /// Crate-internal access to a track's parsed header and resolved path,
    /// powering [`Track`](super::Track)'s provided methods without exposing
    /// [`CmafHeader`] outside the crate.
    pub trait HasHeader {
        /// The track's parsed CMAF header.
        fn header(&self) -> &CmafHeader;
        /// Resolved storage path of the track's CMAF file.
        fn path_str(&self) -> &str;
    }
}

/// The behaviour every parsed CMAF track shares: identity (`id`, `mime_type`),
/// granular access to the parsed header's timing and (sub)segment map, and
/// byte-range reads of its segments. Implemented by [`VideoTrack`],
/// [`AudioTrack`], [`TextTrack`], and the runtime-typed [`AnyTrack`]. Sealed:
/// it cannot be implemented outside this crate.
// In-workspace callers only await the async methods on concrete track values,
// where the compiler proves the returned future Send on its own — the lint's
// "callers cannot add Send bounds" concern doesn't apply here.
#[allow(async_fn_in_trait)]
pub trait Track: sealed::HasHeader {
    /// The representation id: the descriptor's stored id when present, else
    /// derived from the codec fourcc plus per-type discriminators (e.g.
    /// `video_avc1_1080_4807228`, `audio_mp4a_nld_2_196918`, `text_wvtt_eng`).
    /// Manifests and segment routes both key representations by this value,
    /// so the stored id must win on both paths or they drift apart.
    fn id(&self) -> String;

    /// The `video/mp4` / `audio/mp4` / `application/mp4` MIME type of the
    /// track's CMAF segments.
    fn mime_type(&self) -> &'static str;

    /// Resolved storage path of the track's CMAF file (not relative to the
    /// descriptor).
    fn path(&self) -> &str {
        self.path_str()
    }

    /// Units per second for durations in this track.
    fn timescale(&self) -> u32 {
        self.header().timescale
    }

    /// Average bitrate in bits/s, derived from the segment sizes and duration.
    fn bandwidth(&self) -> u32 {
        self.header().bandwidth
    }

    /// Presentation time of the first (sub)segment, in the track timescale.
    fn earliest_presentation_time(&self) -> u64 {
        self.header().earliest_presentation_time
    }

    /// The track's (sub)segments, in presentation order.
    fn segments(&self) -> &[Segment] {
        &self.header().segments
    }

    /// This track's total presentation duration, in milliseconds.
    fn duration_ms(&self) -> u64 {
        units_to_ms(self.header().duration, self.header().timescale)
    }

    /// The longest (sub)segment in this track, in milliseconds (0 if it has
    /// none).
    fn max_segment_duration_ms(&self) -> u64 {
        self.segments()
            .iter()
            .map(|s| s.duration_ms)
            .max()
            .unwrap_or(0)
    }

    /// Read the init segment (`ftyp`+`moov`) bytes through `op`.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    async fn init_segment_bytes(&self, op: &Operator) -> Result<Bytes, CoreError> {
        let s = self.header().init_segment;
        cmaf::read_range(op, self.path_str(), s.offset, s.size).await
    }

    /// Read the media (sub)segment starting at presentation `time` through
    /// `op`, or `None` if no segment starts exactly there.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    async fn segment_bytes(&self, op: &Operator, time: u64) -> Result<Option<Bytes>, CoreError> {
        let mut t = self.earliest_presentation_time();
        for seg in self.segments() {
            if t == time {
                return Ok(Some(
                    cmaf::read_range(op, self.path_str(), seg.offset, seg.size).await?,
                ));
            }
            t += seg.duration;
        }
        Ok(None)
    }
}

/// A parsed CMAF video track: codec, dimensions, and frame rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTrack {
    path: String,
    cmaf_header: CmafHeader,
    cmaf_metadata: VideoCmafMetadata,
    id_descriptor: Option<String>,
}

impl VideoTrack {
    /// Assemble a video track from its resolved `path`, parsed `cmaf_header`,
    /// probed `cmaf_metadata`, and the descriptor `model` it came from (`None`
    /// when the track was probed without a descriptor entry). Only the
    /// descriptor overrides (the stored id) are kept; the model is not.
    pub fn new(
        path: String,
        cmaf_header: CmafHeader,
        cmaf_metadata: VideoCmafMetadata,
        model: Option<&VideoTrackModel>,
    ) -> VideoTrack {
        VideoTrack {
            path,
            cmaf_header,
            cmaf_metadata,
            id_descriptor: model.map(|m| m.id.clone()),
        }
    }

    /// The decoded video codec and its RFC 6381 parameters.
    pub fn codec(&self) -> &VideoCodec {
        &self.cmaf_metadata.codec
    }

    /// Visual width, in pixels.
    pub fn width(&self) -> u32 {
        self.cmaf_metadata.width
    }

    /// Visual height, in pixels.
    pub fn height(&self) -> u32 {
        self.cmaf_metadata.height
    }

    /// Frame rate as a (numerator, denominator) ratio, in frames per second.
    pub fn frame_rate(&self) -> (u32, u32) {
        self.cmaf_metadata.frame_rate
    }

    /// Project to the wire [`TrackModel`], relativizing the stored (resolved)
    /// path back to a file path relative to the descriptor `descriptor_path`.
    fn to_model(&self, descriptor_path: &str) -> TrackModel {
        TrackModel::Video(VideoTrackModel {
            id: self.id(),
            path: path::relativize(descriptor_path, &self.path),
            fourcc: self.codec().fourcc().to_string(),
            timescale: self.timescale(),
            width: self.width(),
            height: self.height(),
        })
    }
}

impl sealed::HasHeader for VideoTrack {
    fn header(&self) -> &CmafHeader {
        &self.cmaf_header
    }

    fn path_str(&self) -> &str {
        &self.path
    }
}

impl Track for VideoTrack {
    fn id(&self) -> String {
        match &self.id_descriptor {
            Some(id) => id.clone(),
            None => format!(
                "video_{}_{}_{}",
                self.codec().fourcc(),
                self.height(),
                self.bandwidth()
            ),
        }
    }

    fn mime_type(&self) -> &'static str {
        "video/mp4"
    }
}

/// A parsed CMAF audio track: codec, sample rate, channels, and language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTrack {
    path: String,
    cmaf_header: CmafHeader,
    cmaf_metadata: AudioCmafMetadata,
    id_descriptor: Option<String>,
}

impl AudioTrack {
    /// Assemble an audio track from its resolved `path`, parsed `cmaf_header`,
    /// probed `cmaf_metadata`, and the descriptor `model` it came from (`None`
    /// when the track was probed without a descriptor entry). Only the
    /// descriptor overrides (the stored id) are kept; the model is not.
    pub fn new(
        path: String,
        cmaf_header: CmafHeader,
        cmaf_metadata: AudioCmafMetadata,
        model: Option<&AudioTrackModel>,
    ) -> AudioTrack {
        AudioTrack {
            path,
            cmaf_header,
            cmaf_metadata,
            id_descriptor: model.map(|m| m.id.clone()),
        }
    }

    /// The decoded audio codec and its RFC 6381 parameters.
    pub fn codec(&self) -> &AudioCodec {
        &self.cmaf_metadata.codec
    }

    /// Sampling rate, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.cmaf_metadata.sample_rate
    }

    /// Number of audio channels (e.g. 2 for stereo, 6 for 5.1).
    pub fn channels(&self) -> u16 {
        self.cmaf_metadata.channels
    }

    /// ISO-639-2 language code (`"und"` when the file leaves it unspecified).
    pub fn language(&self) -> &str {
        &self.cmaf_metadata.language
    }

    /// Project to the wire [`TrackModel`], relativizing the stored (resolved)
    /// path back to a file path relative to the descriptor `descriptor_path`.
    fn to_model(&self, descriptor_path: &str) -> TrackModel {
        TrackModel::Audio(AudioTrackModel {
            id: self.id(),
            path: path::relativize(descriptor_path, &self.path),
            fourcc: self.codec().fourcc().to_string(),
            timescale: self.timescale(),
            sample_rate: self.sample_rate(),
            channels: self.channels(),
            language: Some(self.cmaf_metadata.language.clone()),
        })
    }
}

impl sealed::HasHeader for AudioTrack {
    fn header(&self) -> &CmafHeader {
        &self.cmaf_header
    }

    fn path_str(&self) -> &str {
        &self.path
    }
}

impl Track for AudioTrack {
    fn id(&self) -> String {
        match &self.id_descriptor {
            Some(id) => id.clone(),
            None => format!(
                "audio_{}_{}_{}_{}",
                self.codec().fourcc(),
                self.language(),
                self.channels(),
                self.bandwidth()
            ),
        }
    }

    fn mime_type(&self) -> &'static str {
        "audio/mp4"
    }
}

/// A parsed CMAF timed-text track: codec and language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTrack {
    path: String,
    cmaf_header: CmafHeader,
    cmaf_metadata: TextCmafMetadata,
    id_descriptor: Option<String>,
    language_descriptor: Option<String>,
}

impl TextTrack {
    /// Assemble a text track from its resolved `path`, parsed `cmaf_header`,
    /// probed `cmaf_metadata`, and the descriptor `model` it came from (`None`
    /// when the track was probed without a descriptor entry). Only the
    /// descriptor overrides are kept, the model is not: the stored id, and the
    /// non-empty `language` (a hand-edited empty string falls through to the
    /// probed value).
    pub fn new(
        path: String,
        cmaf_header: CmafHeader,
        cmaf_metadata: TextCmafMetadata,
        model: Option<&TextTrackModel>,
    ) -> TextTrack {
        TextTrack {
            path,
            cmaf_header,
            cmaf_metadata,
            id_descriptor: model.map(|m| m.id.clone()),
            language_descriptor: model
                .and_then(|m| (!m.language.is_empty()).then(|| m.language.clone())),
        }
    }

    /// The decoded text codec and its RFC 6381 parameters.
    pub fn codec(&self) -> &TextCodec {
        &self.cmaf_metadata.codec
    }

    /// The track's effective ISO-639-2 language: the descriptor's value when
    /// present, else the language parsed from the file, else `"und"`.
    pub fn language(&self) -> &str {
        self.language_descriptor
            .as_deref()
            .or(self.cmaf_metadata.language.as_deref())
            .unwrap_or("und")
    }

    /// Project to the wire [`TrackModel`], relativizing the stored (resolved)
    /// path back to a file path relative to the descriptor `descriptor_path`.
    fn to_model(&self, descriptor_path: &str) -> TrackModel {
        TrackModel::Text(TextTrackModel {
            id: self.id(),
            path: path::relativize(descriptor_path, &self.path),
            fourcc: self.codec().fourcc().to_string(),
            timescale: self.timescale(),
            language: self.language().to_string(),
        })
    }
}

impl sealed::HasHeader for TextTrack {
    fn header(&self) -> &CmafHeader {
        &self.cmaf_header
    }

    fn path_str(&self) -> &str {
        &self.path
    }
}

impl Track for TextTrack {
    fn id(&self) -> String {
        match &self.id_descriptor {
            Some(id) => id.clone(),
            None => format!("text_{}_{}", self.codec().fourcc(), self.language()),
        }
    }

    fn mime_type(&self) -> &'static str {
        "application/mp4"
    }
}

/// A parsed CMAF track whose media type is known only at runtime (e.g.
/// resolved from a descriptor id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnyTrack {
    /// A video track.
    Video(VideoTrack),
    /// An audio track.
    Audio(AudioTrack),
    /// A timed-text track.
    Text(TextTrack),
}

impl AnyTrack {
    /// Build the track by parsing the CMAF header at `model`'s path (resolved
    /// against the descriptor's own `descriptor_path`) through `op`. The
    /// per-type model is handed to the matching track's `new`, which lifts the
    /// descriptor overrides out of it (see e.g. [`TextTrack::new`]).
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from reading or parsing the track, or a
    /// [`CoreError::Container`] if the file's media type contradicts the model
    /// (e.g. a video model whose CMAF parses as audio), which means the
    /// descriptor and its file have drifted apart.
    pub async fn from_model(
        op: &Operator,
        model: &TrackModel,
        descriptor_path: &str,
    ) -> Result<AnyTrack, CoreError> {
        let path = path::resolve(descriptor_path, model.path());
        let (cmaf_header, cmaf_metadata) = cmaf::probe(op, &path).await?;
        match (model, cmaf_metadata) {
            (TrackModel::Video(v), CmafMetadata::Video(m)) => Ok(AnyTrack::Video(VideoTrack::new(
                path,
                cmaf_header,
                m,
                Some(v),
            ))),
            (TrackModel::Audio(a), CmafMetadata::Audio(m)) => Ok(AnyTrack::Audio(AudioTrack::new(
                path,
                cmaf_header,
                m,
                Some(a),
            ))),
            (TrackModel::Text(t), CmafMetadata::Text(m)) => Ok(AnyTrack::Text(TextTrack::new(
                path,
                cmaf_header,
                m,
                Some(t),
            ))),
            _ => Err(CoreError::Container(format!(
                "track at {path}: descriptor type and CMAF type disagree"
            ))),
        }
    }
}

impl sealed::HasHeader for AnyTrack {
    fn header(&self) -> &CmafHeader {
        match self {
            AnyTrack::Video(t) => &t.cmaf_header,
            AnyTrack::Audio(t) => &t.cmaf_header,
            AnyTrack::Text(t) => &t.cmaf_header,
        }
    }

    fn path_str(&self) -> &str {
        match self {
            AnyTrack::Video(t) => &t.path,
            AnyTrack::Audio(t) => &t.path,
            AnyTrack::Text(t) => &t.path,
        }
    }
}

impl Track for AnyTrack {
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
}

impl Asset {
    /// An empty asset: no tracks, empty source path.
    pub fn new() -> Asset {
        Asset::default()
    }

    /// Parse the CMAF file at `file_path` and add it as the video, audio, or
    /// text track its `hdlr` box declares. `descriptor_path` resolves
    /// `file_path` relative to the descriptor. Tracks carry no ordering
    /// guarantee.
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
            CmafMetadata::Video(m) => {
                self.video_tracks
                    .push(VideoTrack::new(path, cmaf_header, m, None))
            }
            CmafMetadata::Audio(m) => {
                self.audio_tracks
                    .push(AudioTrack::new(path, cmaf_header, m, None))
            }
            CmafMetadata::Text(m) => {
                self.text_tracks
                    .push(TextTrack::new(path, cmaf_header, m, None))
            }
        }
        Ok(())
    }

    /// File a parsed track under the vec for its media type.
    fn push_track(&mut self, track: AnyTrack) {
        match track {
            AnyTrack::Video(t) => self.video_tracks.push(t),
            AnyTrack::Audio(t) => self.audio_tracks.push(t),
            AnyTrack::Text(t) => self.text_tracks.push(t),
        }
    }

    /// Build an [`Asset`] from its wire [`AssetModel`], parsing every track's
    /// CMAF header. Tracks are independent, so all are probed concurrently.
    /// `path` is the descriptor's own path, used to resolve each track's
    /// relative path.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from reading or parsing a track, including
    /// the [`CoreError::Container`] mismatch when a file's media type
    /// contradicts its descriptor entry (see [`AnyTrack::from_model`]).
    pub async fn from_model(
        op: &Operator,
        model: AssetModel,
        path: impl Into<String>,
    ) -> Result<Asset, CoreError> {
        let path = path.into();
        let tracks = try_join_all(
            model
                .tracks
                .iter()
                .map(|track| AnyTrack::from_model(op, track, &path)),
        )
        .await?;
        let mut asset = Asset::new();
        for track in tracks {
            asset.push_track(track);
        }
        asset.path = path;
        Ok(asset)
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

    fn header(timescale: u32, duration: u64, seg_durations: &[u64]) -> CmafHeader {
        CmafHeader {
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
        }
    }

    fn text_model(id: &str, language: &str) -> TextTrackModel {
        TextTrackModel {
            id: id.to_string(),
            path: String::new(),
            fourcc: "wvtt".to_string(),
            timescale: 1000,
            language: language.to_string(),
        }
    }

    fn text_track(language: Option<&str>, model: Option<&TextTrackModel>) -> TextTrack {
        TextTrack::new(
            String::new(),
            header(1000, 0, &[]),
            TextCmafMetadata {
                codec: TextCodec::Wvtt,
                language: language.map(str::to_string),
            },
            model,
        )
    }

    #[test]
    fn text_track_id_is_text_fourcc_language() {
        assert_eq!(text_track(Some("und"), None).id(), "text_wvtt_und");
    }

    #[test]
    fn text_language_falls_back_to_und_when_file_has_none() {
        assert_eq!(text_track(None, None).language(), "und");
    }

    #[test]
    fn descriptor_language_wins_over_probed_language() {
        let model = text_model("text_wvtt_nld", "nld");
        assert_eq!(text_track(Some("eng"), Some(&model)).language(), "nld");
    }

    #[test]
    fn empty_descriptor_language_falls_through_to_probed_language() {
        let model = text_model("text_wvtt_eng", "");
        assert_eq!(text_track(Some("eng"), Some(&model)).language(), "eng");
    }

    #[test]
    fn probed_language_applies_without_a_descriptor() {
        assert_eq!(text_track(Some("eng"), None).language(), "eng");
    }

    #[test]
    fn descriptor_id_wins_over_derived_id() {
        let model = text_model("my_subs", "nld");
        assert_eq!(text_track(Some("eng"), Some(&model)).id(), "my_subs");
    }

    #[test]
    fn to_model_round_trips_the_descriptor_id_and_language() {
        let model = text_model("text_wvtt_nld", "nld");
        let track = text_track(Some("eng"), Some(&model));
        let TrackModel::Text(m) = track.to_model("asset.json") else {
            panic!("expected a text model");
        };
        assert_eq!(m.language, "nld");
        assert_eq!(m.id, "text_wvtt_nld");
    }

    fn video_track(timescale: u32, duration: u64, seg_durations: &[u64]) -> VideoTrack {
        VideoTrack::new(
            String::new(),
            header(timescale, duration, seg_durations),
            VideoCmafMetadata {
                codec: VideoCodec::Avc {
                    profile: 0,
                    constraints: 0,
                    level: 0,
                },
                width: 0,
                height: 0,
                frame_rate: (0, 1),
            },
            None,
        )
    }

    #[test]
    fn video_descriptor_id_wins_over_derived_id() {
        let model = VideoTrackModel {
            id: "my_video".to_string(),
            path: String::new(),
            fourcc: "avc1".to_string(),
            timescale: 90_000,
            width: 0,
            height: 0,
        };
        let track = VideoTrack::new(
            String::new(),
            header(90_000, 0, &[]),
            VideoCmafMetadata {
                codec: VideoCodec::Avc {
                    profile: 0,
                    constraints: 0,
                    level: 0,
                },
                width: 0,
                height: 0,
                frame_rate: (0, 1),
            },
            Some(&model),
        );
        assert_eq!(track.id(), "my_video");
    }

    #[test]
    fn audio_descriptor_id_wins_over_derived_id() {
        let model = AudioTrackModel {
            id: "my_audio".to_string(),
            path: String::new(),
            fourcc: "mp4a".to_string(),
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            language: Some("nld".to_string()),
        };
        let track = AudioTrack::new(
            String::new(),
            header(48_000, 0, &[]),
            AudioCmafMetadata {
                codec: AudioCodec::Aac {
                    audio_object_type: 2,
                },
                sample_rate: 48_000,
                channels: 2,
                language: "nld".to_string(),
            },
            Some(&model),
        );
        assert_eq!(track.id(), "my_audio");
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

    #[test]
    fn segmentation_accepts_ascending_boundaries() {
        let s = Segmentation::new(Some(3000), vec![1000, 2000]).unwrap();
        assert_eq!(s.min_segment_length_ms, Some(3000));
        assert_eq!(s.segment_boundaries_ms, vec![1000, 2000]);
    }

    #[test]
    fn segmentation_accepts_empty_boundaries_and_no_min() {
        assert_eq!(
            Segmentation::new(None, Vec::new()).unwrap(),
            Segmentation::default()
        );
    }

    #[test]
    fn segmentation_rejects_unsorted_boundaries() {
        assert!(Segmentation::new(None, vec![2000, 1000]).is_err());
    }

    #[test]
    fn segmentation_rejects_duplicate_boundaries() {
        assert!(Segmentation::new(None, vec![1000, 1000]).is_err());
    }

    #[test]
    fn segmentation_rejects_a_zero_boundary() {
        assert!(Segmentation::new(None, vec![0, 1000]).is_err());
    }
}
