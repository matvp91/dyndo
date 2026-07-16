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
    /// Minimum length of a served segment, in milliseconds, from the
    /// descriptor. `None` (or 0) serves each CMAF fragment as its own segment.
    pub min_segment_length_ms: Option<u64>,
    /// Splice points, in milliseconds from the start of the presentation,
    /// from the descriptor. A served segment never spans one.
    pub segment_boundaries_ms: Vec<u64>,
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

/// Group a track's raw CMAF fragments into served segments. Splice points
/// (`segment_boundaries_ms`) partition the fragments — a served segment never
/// spans one — and within each partition whole fragments are accumulated
/// greedily until a group reaches `min_segment_length_ms`. The final group of
/// a partition may be shorter (short tail). Fragments are contiguous byte
/// ranges, so a group is the first fragment's offset with summed sizes and
/// durations. All comparisons are exact u128 integer math (`units * 1000` vs
/// `ms * timescale`); summing `duration_ms` keeps per-track totals drift-free
/// because group boundaries are a subset of the raw drift-free boundaries.
fn group_segments(
    raw: &[Segment],
    timescale: u32,
    segment_boundaries_ms: Option<&[u64]>,
    min_segment_length_ms: Option<u64>,
) -> Vec<Segment> {
    let min_ms = match min_segment_length_ms {
        Some(ms) if ms > 0 => ms,
        _ => return raw.to_vec(),
    };
    let min_target = min_ms as u128 * timescale as u128;

    // cum[i] = presentation units before fragment i; cum[raw.len()] = total.
    let mut cum = Vec::with_capacity(raw.len() + 1);
    cum.push(0u64);
    for s in raw {
        cum.push(cum[cum.len() - 1] + s.duration);
    }
    let cuts = snap_cuts(&cum, timescale, segment_boundaries_ms.unwrap_or(&[]));

    let mut out = Vec::new();
    let mut start = 0;
    let mut next_cut = 0;
    for end in 1..=raw.len() {
        while next_cut < cuts.len() && cuts[next_cut] <= start {
            next_cut += 1;
        }
        let group_units = cum[end] - cum[start];
        let long_enough = group_units as u128 * 1000 >= min_target;
        let at_cut = next_cut < cuts.len() && cuts[next_cut] == end;
        if long_enough || at_cut || end == raw.len() {
            out.push(Segment {
                offset: raw[start].offset,
                size: raw[start..end].iter().map(|s| s.size).sum(),
                duration: group_units,
                duration_ms: raw[start..end].iter().map(|s| s.duration_ms).sum(),
            });
            start = end;
        }
    }
    out
}

/// Snap each splice point to the nearest fragment boundary, returned as
/// ascending, deduplicated indices into the cumulative-units table `cum`
/// (index 0 = track start — a no-op cut, as is `cum.len() - 1`, the track
/// end). Exact integer comparison in u128; a tie snaps earlier. Splice points
/// are a set: order and duplicates in the descriptor don't matter, so the
/// result is sorted rather than the input rejected.
fn snap_cuts(cum: &[u64], timescale: u32, boundaries_ms: &[u64]) -> Vec<usize> {
    let mut cuts: Vec<usize> = boundaries_ms
        .iter()
        .map(|&splice_ms| {
            let target = splice_ms as u128 * timescale as u128;
            let i = cum.partition_point(|&c| (c as u128) * 1000 < target);
            if i == 0 {
                0
            } else if i == cum.len() {
                cum.len() - 1
            } else {
                let below = target - cum[i - 1] as u128 * 1000;
                let above = cum[i] as u128 * 1000 - target;
                if below <= above { i - 1 } else { i }
            }
        })
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
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

    /// The track's served (sub)segments, in presentation order: the raw CMAF
    /// fragments grouped to at least `min_segment_length_ms`, never across a
    /// splice point in `segment_boundaries_ms` (both `None` = one segment per
    /// fragment). Manifest builders and the segment route must receive the
    /// same policy or advertised segment times will not resolve.
    fn segments(
        &self,
        segment_boundaries_ms: Option<&[u64]>,
        min_segment_length_ms: Option<u64>,
    ) -> Vec<Segment> {
        group_segments(
            &self.header().segments,
            self.timescale(),
            segment_boundaries_ms,
            min_segment_length_ms,
        )
    }

    /// This track's total presentation duration, in milliseconds.
    fn duration_ms(&self) -> u64 {
        units_to_ms(self.header().duration, self.header().timescale)
    }

    /// The longest served (sub)segment in this track, in milliseconds (0 if it
    /// has none), under the same grouping policy as [`Track::segments`].
    fn max_segment_duration_ms(
        &self,
        segment_boundaries_ms: Option<&[u64]>,
        min_segment_length_ms: Option<u64>,
    ) -> u64 {
        self.segments(segment_boundaries_ms, min_segment_length_ms)
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
    /// `op`, or `None` if no segment starts exactly there. `time` is matched
    /// against the served segments — pass the same grouping policy the
    /// manifest was built with.
    ///
    /// # Errors
    /// Propagates any [`CoreError`] from the underlying read.
    async fn segment_bytes(
        &self,
        op: &Operator,
        time: u64,
        segment_boundaries_ms: Option<&[u64]>,
        min_segment_length_ms: Option<u64>,
    ) -> Result<Option<Bytes>, CoreError> {
        let mut t = self.earliest_presentation_time();
        for seg in self.segments(segment_boundaries_ms, min_segment_length_ms) {
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
        asset.min_segment_length_ms = model.min_segment_length_ms;
        asset.segment_boundaries_ms = model.segment_boundaries_ms;
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
            min_segment_length_ms: asset.min_segment_length_ms,
            segment_boundaries_ms: asset.segment_boundaries_ms.clone(),
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
            video_track(48_000, 0, &[48_000, 96_000, 24_000]).max_segment_duration_ms(None, None),
            2000
        );
    }

    #[test]
    fn max_segment_duration_ms_is_zero_without_segments() {
        assert_eq!(
            video_track(48_000, 0, &[]).max_segment_duration_ms(None, None),
            0
        );
    }

    #[test]
    fn asset_round_trips_the_grouping_fields_into_the_model() {
        let mut asset = Asset::new();
        asset.min_segment_length_ms = Some(3000);
        asset.segment_boundaries_ms = vec![683640];
        let model = AssetModel::from(&asset);
        assert_eq!(model.min_segment_length_ms, Some(3000));
        assert_eq!(model.segment_boundaries_ms, vec![683640]);
    }

    #[tokio::test]
    async fn from_model_carries_the_grouping_fields_onto_the_asset() {
        use opendal::services::Fs;

        // With no tracks, from_model performs no I/O — any operator works.
        let dir = tempfile::tempdir().unwrap();
        let op = Operator::new(Fs::default().root(dir.path().to_str().unwrap())).unwrap();
        let model = AssetModel {
            min_segment_length_ms: Some(3000),
            segment_boundaries_ms: vec![683640],
            tracks: Vec::new(),
        };
        let asset = Asset::from_model(&op, model, "asset.json").await.unwrap();
        assert_eq!(asset.min_segment_length_ms, Some(3000));
        assert_eq!(asset.segment_boundaries_ms, vec![683640]);
    }

    /// A contiguous fragment run: `durations` in timescale units, each fragment
    /// 10 bytes, starting at byte 100 (mirroring data after an init segment).
    fn frags(timescale: u32, durations: &[u64]) -> Vec<Segment> {
        let mut offset = 100;
        let (mut acc_units, mut acc_ms) = (0u64, 0u64);
        durations
            .iter()
            .map(|&d| {
                acc_units += d;
                let boundary_ms = units_to_ms(acc_units, timescale);
                let s = Segment {
                    offset,
                    size: 10,
                    duration: d,
                    duration_ms: boundary_ms - acc_ms,
                };
                acc_ms = boundary_ms;
                offset += 10;
                s
            })
            .collect()
    }

    #[test]
    fn grouping_is_a_noop_without_a_minimum() {
        let raw = frags(90000, &[172800, 172800]);
        assert_eq!(group_segments(&raw, 90000, None, None), raw);
        assert_eq!(group_segments(&raw, 90000, None, Some(0)), raw);
    }

    #[test]
    fn a_minimum_below_every_fragment_is_a_noop() {
        // 1.92s GOPs, min 0.5s: every fragment already satisfies the minimum.
        let raw = frags(90000, &[172800; 3]);
        assert_eq!(group_segments(&raw, 90000, None, Some(500)), raw);
    }

    #[test]
    fn grouping_pairs_fragments_to_reach_the_minimum() {
        // 1.92s GOPs, min 3s -> 3.84s pairs.
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, None, Some(3000));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].duration, 345600);
        assert_eq!(out[0].offset, 100);
        assert_eq!(out[0].size, 20);
        assert_eq!(out[1].offset, 120);
    }

    #[test]
    fn a_group_closes_exactly_at_the_minimum() {
        // 1s fragments @ min 2s -> exact 2s groups, not 3s.
        let raw = frags(90000, &[90000; 4]);
        let out = group_segments(&raw, 90000, None, Some(2000));
        assert_eq!(
            out.iter().map(|s| s.duration).collect::<Vec<_>>(),
            vec![180000, 180000]
        );
    }

    #[test]
    fn the_track_tail_may_be_shorter_than_the_minimum() {
        let raw = frags(90000, &[172800, 172800, 122400]);
        let out = group_segments(&raw, 90000, None, Some(3000));
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].duration, 122400); // 1.36s tail, alone
    }

    #[test]
    fn a_minimum_beyond_the_track_makes_one_segment() {
        let raw = frags(90000, &[172800; 3]);
        let out = group_segments(&raw, 90000, None, Some(3_600_000));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].duration, 518400);
        assert_eq!(out[0].size, 30);
    }

    #[test]
    fn grouping_an_empty_track_yields_nothing() {
        assert!(group_segments(&[], 90000, None, Some(3000)).is_empty());
    }

    #[test]
    fn grouped_duration_ms_stays_drift_free() {
        // timescale 3, six 1-unit fragments (1/3s each), min 600ms -> groups of
        // 2 (2/3s = 666.67ms exact). Drift-free totals: 666+667+667 = 2000.
        let raw = frags(3, &[1, 1, 1, 1, 1, 1]);
        let out = group_segments(&raw, 3, None, Some(600));
        let ms: Vec<u64> = out.iter().map(|s| s.duration_ms).collect();
        assert_eq!(ms.iter().sum::<u64>(), 2000);
        assert_eq!(ms, vec![666, 667, 667]);
    }

    #[test]
    fn a_group_never_crosses_a_splice_point() {
        // Four 1.92s GOPs, splice exactly at 3.84s: without the cut min 5s
        // would span it; with it, both partitions close at the splice.
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, Some(&[3840]), Some(5000));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].duration, 345600);
        assert_eq!(out[1].duration, 345600);
    }

    #[test]
    fn a_conditioning_fragment_before_a_splice_becomes_a_short_segment() {
        // The real-asset shape scaled down: 2x1.92s + 0.12s | splice | 1.8s + 1.92s.
        let raw = frags(90000, &[172800, 172800, 10800, 162000, 172800]);
        let out = group_segments(&raw, 90000, Some(&[3960]), Some(3000));
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![345600, 10800, 334800]); // 3.84s, 0.12s tail, 3.72s
    }

    #[test]
    fn a_splice_snaps_to_the_nearest_fragment_boundary() {
        // Boundaries at 0/1.92/3.84s; splice at 2.0s is nearer 1.92 than 3.84.
        let raw = frags(90000, &[172800, 172800]);
        let out = group_segments(&raw, 90000, Some(&[2000]), Some(4000));
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![172800, 172800]);
    }

    #[test]
    fn an_exact_tie_snaps_earlier() {
        // 2s fragments; splice at 3.0s ties between 2s and 4s -> snaps to 2s.
        let raw = frags(90000, &[180000, 180000, 180000]);
        let out = group_segments(&raw, 90000, Some(&[3000]), Some(10_000));
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![180000, 360000]);
    }

    #[test]
    fn a_splice_beyond_the_track_is_a_noop() {
        let raw = frags(90000, &[172800; 2]);
        let out = group_segments(&raw, 90000, Some(&[999_000]), Some(5000));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn two_splices_snapping_to_one_boundary_cut_once() {
        // Both 1.90s and 1.94s snap to the 1.92s boundary.
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, Some(&[1900, 1940]), Some(5000));
        let durs: Vec<u64> = out.iter().map(|s| s.duration).collect();
        assert_eq!(durs, vec![172800, 518400]);
    }

    #[test]
    fn boundary_order_does_not_matter() {
        // Splice points are a set; unsorted input cuts at the same points.
        let raw = frags(90000, &[172800; 6]);
        let sorted = group_segments(&raw, 90000, Some(&[1920, 5760]), Some(9000));
        let unsorted = group_segments(&raw, 90000, Some(&[5760, 1920]), Some(9000));
        assert_eq!(sorted, unsorted);
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn boundaries_without_a_minimum_leave_fragments_as_is() {
        let raw = frags(90000, &[172800; 4]);
        let out = group_segments(&raw, 90000, Some(&[3840]), None);
        assert_eq!(out, raw);
    }

    #[test]
    fn video_and_text_group_to_identical_boundary_times() {
        // Text durations are the whole-ms mirror of the video durations;
        // grouping decisions must match so DASH/HLS timelines stay aligned
        // across tracks.
        let video = frags(90000, &[172800, 172800, 10800, 162000, 172800]);
        let text = frags(1000, &[1920, 1920, 120, 1800, 1920]);
        let p = |ts_frags: &[Segment], ts: u32| -> Vec<u128> {
            let mut acc = 0u128;
            group_segments(ts_frags, ts, Some(&[3960]), Some(3000))
                .iter()
                .map(|s| {
                    acc += s.duration as u128 * 1000 / ts as u128;
                    acc
                })
                .collect()
        };
        assert_eq!(p(&video, 90000), p(&text, 1000));
    }
}
