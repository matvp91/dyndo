use bytes::Bytes;
use opendal::Operator;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::image::experimental_sprite_generator::{ExperimentalSpriteGenerator, FrameSelection};
use crate::track::cmaf::{CmafKind, ResolvedCmafTrack};

const BITS_PER_PIXEL: u64 = 1;

/// A thumbnail track generated from source video when requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThumbnailTrack {
    pub id: String,
    pub tile_size: u32,
    pub width: u32,
    pub step: u32,
}

impl ThumbnailTrack {
    pub fn new(id: String, tile_size: u32, width: u32, step: u32) -> Self {
        Self {
            id,
            tile_size,
            width,
            step,
        }
    }

    /// Resolves this thumbnail configuration against available CMAF tracks.
    pub fn resolve<'a>(
        &self,
        tracks: impl IntoIterator<Item = &'a ResolvedCmafTrack>,
    ) -> Option<ResolvedThumbnailTrack> {
        let source = select_source(tile_width(self), tracks)?;
        let (_, height) = dimensions(self, source)?;

        Some(ResolvedThumbnailTrack {
            track: self.clone(),
            source: source.clone(),
            height,
        })
    }
}

/// An error encountered while generating thumbnail media.
#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("could not generate thumbnail sprite")]
    SpriteGenerator,
}

/// A resolved thumbnail track.
#[derive(Clone)]
pub struct ResolvedThumbnailTrack {
    track: ThumbnailTrack,
    source: ResolvedCmafTrack,
    height: u32,
}

impl ResolvedThumbnailTrack {
    /// Returns the video track selected to produce this thumbnail sprite.
    pub fn source(&self) -> &ResolvedCmafTrack {
        &self.source
    }

    /// Returns the thumbnail track identifier.
    pub fn id(&self) -> &str {
        &self.track.id
    }

    /// Returns the number of tiles in each sprite row and column.
    pub fn tile_size(&self) -> u32 {
        self.track.tile_size
    }

    /// Returns the interval between adjacent thumbnail frames, in milliseconds.
    pub fn step(&self) -> u32 {
        self.track.step
    }

    /// Returns the width of the complete thumbnail sprite.
    pub fn width(&self) -> u32 {
        self.track.width
    }

    /// Returns the height of the complete thumbnail sprite.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the dimensions of one thumbnail tile.
    pub fn tile_dimensions(&self) -> (u32, u32) {
        let tile_size = self.track.tile_size.max(1);
        (self.width() / tile_size, self.height / tile_size)
    }

    /// Returns the duration covered by one thumbnail sprite, in milliseconds.
    pub fn sprite_duration(&self) -> u64 {
        u64::from(self.track.tile_size)
            .saturating_mul(u64::from(self.track.tile_size))
            .saturating_mul(u64::from(self.track.step))
    }

    /// Returns the estimated bandwidth of the JPEG sprite representation.
    pub fn bandwidth(&self) -> u64 {
        let bits = u128::from(self.width())
            .saturating_mul(u128::from(self.height))
            .saturating_mul(u128::from(BITS_PER_PIXEL));
        let bits_per_second = bits
            .saturating_mul(1_000)
            .div_ceil(u128::from(self.sprite_duration()).max(1));

        u64::try_from(bits_per_second).unwrap_or(u64::MAX).max(1)
    }

    /// Generates a thumbnail sprite beginning at `time`.
    ///
    /// Returns `None` when thumbnails are disabled or unavailable for the track.
    ///
    /// # Errors
    ///
    /// Returns an error when a frame cannot be read, decoded, composed, or encoded.
    pub async fn jpeg(&self, op: &Operator, time: u64) -> Result<Option<Bytes>, ThumbnailError> {
        let (Some(first), Some(last)) = (
            self.source.segments().first(),
            self.source.segments().last(),
        ) else {
            return Ok(None);
        };
        let Some(start) = first.start_time().checked_add(time) else {
            return Ok(None);
        };
        let end = last.end_time();
        if start >= end {
            return Ok(None);
        }

        let (tile_width, tile_height) = self.tile_dimensions();
        let times: Vec<_> = self.frame_times(start, end).collect();
        let jpeg = ExperimentalSpriteGenerator::new(op, &self.source)
            .jpeg(
                &times,
                self.track.tile_size,
                tile_width,
                tile_height,
                FrameSelection::PreviousKeyframe,
            )
            .await
            .map_err(|_| ThumbnailError::SpriteGenerator)?;
        Ok(Some(jpeg))
    }

    fn frame_times(&self, start: u64, end: u64) -> impl Iterator<Item = u64> {
        let step = u64::from(self.track.step);
        (0..u64::from(self.track.tile_size).pow(2))
            .map_while(move |index| start.checked_add(index * step).filter(|time| *time < end))
    }
}

fn select_source<'a>(
    width: u32,
    tracks: impl IntoIterator<Item = &'a ResolvedCmafTrack>,
) -> Option<&'a ResolvedCmafTrack> {
    let mut smallest_suitable = None;
    let mut largest = None;

    for track in tracks {
        let Some((track, video_width)) = video_width(track) else {
            continue;
        };
        if largest.is_none_or(|(_, largest_width)| video_width > largest_width) {
            largest = Some((track, video_width));
        }
        if video_width >= width
            && smallest_suitable.is_none_or(|(_, smallest_width)| video_width < smallest_width)
        {
            smallest_suitable = Some((track, video_width));
        }
    }

    smallest_suitable.or(largest).map(|(track, _)| track)
}

fn video_width(track: &ResolvedCmafTrack) -> Option<(&ResolvedCmafTrack, u32)> {
    let CmafKind::Video(video) = track.kind() else {
        return None;
    };
    Some((track, video.width))
}

fn tile_width(track: &ThumbnailTrack) -> u32 {
    track.width / track.tile_size.max(1)
}

fn dimensions(track: &ThumbnailTrack, source: &ResolvedCmafTrack) -> Option<(u32, u32)> {
    let CmafKind::Video(video) = source.kind() else {
        return None;
    };
    if video.width == 0 {
        return None;
    }
    let height =
        u64::from(track.width).saturating_mul(u64::from(video.height)) / u64::from(video.width);
    let height = if track.tile_size == 0 {
        height
    } else {
        height - height % u64::from(track.tile_size)
    };
    let height = u32::try_from(height).ok()?;
    Some((track.width, height))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::ThumbnailTrack;
    use crate::codec::{CodecConfig, WvttCodec};
    use crate::track::cmaf::{CmafKind, InitSegment, ResolvedCmafTrack};
    use crate::track::metadata::VideoMetadata;

    fn video(id: &str, width: u32, height: u32) -> ResolvedCmafTrack {
        ResolvedCmafTrack::new(
            id.to_string(),
            format!("{id}.mp4").into(),
            CmafKind::Video(VideoMetadata {
                width,
                height,
                frame_rate: "25/1".to_string(),
            }),
            Arc::new(InitSegment::new(CodecConfig::Wvtt(WvttCodec), 1_000, 0, 0)),
            Vec::new(),
        )
    }

    fn track(tile_size: u32, width: u32) -> ThumbnailTrack {
        ThumbnailTrack::new("thumbnail".to_string(), tile_size, width, 1_000)
    }

    #[test]
    fn thumbnail_selects_the_smallest_video_that_meets_the_tile_width() {
        let tracks = [video("720", 1_280, 720), video("1080", 1_920, 1_080)];
        let track = track(2, 1_080);

        let thumbnail = track.resolve(&tracks).unwrap();

        assert_eq!(thumbnail.source().id(), "720");
    }

    #[test]
    fn thumbnail_uses_the_largest_video_when_all_sources_are_too_small() {
        let tracks = [video("720", 1_280, 720), video("1080", 1_920, 1_080)];
        let track = track(4, 8_000);

        let thumbnail = track.resolve(&tracks).unwrap();

        assert_eq!(thumbnail.source().id(), "1080");
    }

    #[test]
    fn thumbnail_preserves_its_track_settings() {
        let configured = track(4, 640);
        let track = video("720", 1_280, 720);
        let thumbnail = configured.resolve([&track]).unwrap();

        assert_eq!(thumbnail.width(), 640);
    }

    #[test]
    fn thumbnail_resolves_with_zero_settings() {
        let configured = ThumbnailTrack::new("thumbnail".to_string(), 0, 0, 0);
        let source = video("720", 1_280, 720);

        let thumbnail = configured.resolve([&source]).unwrap();

        assert_eq!(thumbnail.source().id(), "720");
    }
}
