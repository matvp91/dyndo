use std::ops::Range;

use bytes::Bytes;
use futures_util::future::try_join_all;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};
use uuid::Uuid;

use crate::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
use crate::fragment::Fragment;
use crate::opendal::add_operator_layers;
use crate::probe::{Probe, ProbeError};
use crate::segment::SegmentOptions;

#[derive(Debug, thiserror::Error)]
pub enum TrackError {
    #[error(transparent)]
    Probe(#[from] ProbeError),
    #[error(transparent)]
    Storage(#[from] opendal::Error),
}

pub struct Track {
    id: String,
    path: RelativePathBuf,
    codec: String,
    kind: TrackKind,
    timescale: u32,
    earliest_presentation_time: u64,
    initialization_range: Range<u64>,
    fragments: Vec<Fragment>,
}

impl Track {
    /// Probes the track at `path`, packaging it as CMAF if it is not already.
    ///
    /// A subtitle document is packed into a `wvtt` track as it is read, fragmented
    /// at the splice points and text segment length in `options`, so its fragments
    /// group into segments alongside the asset's other tracks.
    ///
    /// `descriptor` is what the asset already declares about the track: its id, and
    /// the kind whose metadata a probe cannot read off the file. Pass `None` for a
    /// track no descriptor covers yet — it takes a [`generate_id`] id, which is what
    /// indexing seeds a new descriptor with.
    ///
    /// # Errors
    ///
    /// [`TrackError`] if the track cannot be read, packaged, or indexed.
    pub async fn probe(
        op: &Operator,
        path: &RelativePath,
        descriptor: Option<&TrackDescriptor>,
        options: &SegmentOptions,
    ) -> Result<Self, TrackError> {
        let layered = add_operator_layers(op, options);
        let probed = Probe::run(&layered, path).await?;
        let (id, kind) = match descriptor {
            Some(descriptor) => (descriptor.id.clone(), descriptor.kind.clone()),
            None => (generate_id(&probed.kind, path), probed.kind),
        };

        Ok(Self {
            id,
            path: path.to_owned(),
            codec: probed.codec,
            kind,
            timescale: probed.timescale,
            earliest_presentation_time: probed.earliest_presentation_time,
            initialization_range: probed.initialization_range,
            fragments: probed.fragments,
        })
    }

    /// Probes every track declared by `asset` concurrently, packaging subtitle
    /// documents as the asset's segment options describe.
    ///
    /// Returned tracks retain descriptor order and use descriptor metadata for
    /// their track kind.
    ///
    /// # Errors
    ///
    /// Returns the first [`TrackError`] encountered while probing the tracks.
    pub async fn probe_all(
        op: &Operator,
        asset: &AssetDescriptor,
    ) -> Result<Vec<Self>, TrackError> {
        let reads = asset.tracks.iter().map(|descriptor| {
            let path = asset.track_path(descriptor);
            async move { Self::probe(op, &path, Some(descriptor), &asset.segment_options).await }
        });

        try_join_all(reads).await
    }

    /// The id this track is addressed by: the one its descriptor declares, or a
    /// generated one when no descriptor covers it yet.
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    pub fn kind(&self) -> &TrackKind {
        &self.kind
    }

    pub fn codec(&self) -> &str {
        &self.codec
    }

    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    /// Returns the track's earliest presentation time in timescale units.
    pub fn earliest_presentation_time(&self) -> u64 {
        self.earliest_presentation_time
    }

    /// Returns the byte range containing the track's CMAF initialization segment.
    pub fn initialization_range(&self) -> Range<u64> {
        self.initialization_range.clone()
    }

    pub(crate) fn fragments(&self) -> &[Fragment] {
        &self.fragments
    }

    /// Returns the total duration of the track's fragments in milliseconds.
    pub fn duration(&self) -> u32 {
        let raw_duration: u128 = self
            .fragments
            .iter()
            .map(|fragment| u128::from(fragment.raw_duration()))
            .sum();
        // Rounded down: a presentation claiming more than it holds points a player
        // past the end of the media, while claiming less only leaves a little unread.
        let duration = raw_duration * 1000 / u128::from(self.timescale);
        u32::try_from(duration).unwrap_or(u32::MAX)
    }

    /// Reads a byte range of the track. Pass the `options` it was probed under, so
    /// the packaging — and with it the meaning of the range — is the same.
    pub async fn read_range(
        &self,
        op: &Operator,
        options: &SegmentOptions,
        range: Range<u64>,
    ) -> Result<Bytes, TrackError> {
        let op = add_operator_layers(op, options);

        Ok(op
            .read_with(self.path.as_str())
            .range(range)
            .await?
            .to_bytes())
    }

    /// Reads the track's CMAF initialization segment.
    pub async fn read_initialization(
        &self,
        op: &Operator,
        options: &SegmentOptions,
    ) -> Result<Bytes, TrackError> {
        self.read_range(op, options, self.initialization_range())
            .await
    }
}

/// The id a track takes when no descriptor names one yet, which indexing then
/// records. Derived from the source path, so re-indexing the same file lands on the
/// same id and the manifests keep addressing it by the same URL.
pub fn generate_id(kind: &TrackKind, path: &RelativePath) -> String {
    let hash = Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_str().as_bytes());

    format!("{}_{hash}", kind.content_type())
}

/// Returns the longest video duration in milliseconds, or the longest audio
/// duration when no video track is present. Text tracks do not determine
/// presentation length.
pub fn max_duration(tracks: &[Track]) -> u32 {
    max_matching_duration(tracks, |kind| matches!(kind, TrackKind::Video(_))).unwrap_or_else(|| {
        max_matching_duration(tracks, |kind| matches!(kind, TrackKind::Audio(_))).unwrap_or(0)
    })
}

fn max_matching_duration(tracks: &[Track], include: impl Fn(&TrackKind) -> bool) -> Option<u32> {
    tracks
        .iter()
        .filter(|track| include(track.kind()))
        .map(Track::duration)
        .max()
}

#[cfg(test)]
impl Track {
    pub(crate) fn fake(kind: TrackKind, timescale: u32, fragments: Vec<Fragment>) -> Self {
        Self {
            id: "fake".to_string(),
            path: RelativePathBuf::from("track.mp4"),
            codec: "fake".to_string(),
            kind,
            timescale,
            earliest_presentation_time: 0,
            initialization_range: 0..0,
            fragments,
        }
    }

    pub(crate) fn fake_earliest_presentation_time(mut self, time: u64) -> Self {
        self.earliest_presentation_time = time;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_descriptor::{AudioKind, TextKind, VideoKind};

    #[test]
    fn duration_converts_timescale_units() {
        let track = Track::fake(
            video_kind(),
            90_000,
            vec![Fragment::new(0, 10, 295_200).unwrap()],
        );

        assert_eq!(track.duration(), 3_280);
    }

    #[test]
    fn duration_truncates_fractional_milliseconds() {
        let track = Track::fake(
            video_kind(),
            3_000,
            vec![Fragment::new(0, 10, 3_001).unwrap()],
        );

        assert_eq!(track.duration(), 1_000);
    }

    #[test]
    fn max_duration_prefers_video_over_longer_audio() {
        let tracks = vec![track(audio_kind(), 10_000), track(video_kind(), 4_000)];

        assert_eq!(max_duration(&tracks), 4_000);
    }

    #[test]
    fn max_duration_falls_back_to_audio_without_video() {
        let tracks = vec![track(text_kind(), 20_000), track(audio_kind(), 5_000)];

        assert_eq!(max_duration(&tracks), 5_000);
    }

    #[test]
    fn max_duration_ignores_text_only_assets() {
        let tracks = vec![track(text_kind(), 20_000)];

        assert_eq!(max_duration(&tracks), 0);
    }

    fn track(kind: TrackKind, raw_duration: u32) -> Track {
        Track::fake(
            kind,
            1_000,
            vec![Fragment::new(0, 10, raw_duration).unwrap()],
        )
    }

    fn video_kind() -> TrackKind {
        TrackKind::Video(VideoKind {
            width: 1920,
            height: 1080,
            frame_rate: "25/1".to_string(),
        })
    }

    fn audio_kind() -> TrackKind {
        TrackKind::Audio(AudioKind {
            sample_rate: 48_000,
            channels: 2,
            language: "eng".parse().unwrap(),
            role: None,
        })
    }

    fn text_kind() -> TrackKind {
        TrackKind::Text(TextKind {
            language: "eng".parse().unwrap(),
            role: None,
        })
    }
}
