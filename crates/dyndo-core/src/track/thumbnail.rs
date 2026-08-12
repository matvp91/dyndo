use bytes::Bytes;
use opendal::Operator;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::image::Sprite;
use crate::track::cmaf::{CmafKind, ResolvedCmafTrack};

/// A thumbnail track generated from source video when requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThumbnailTrack {
    pub id: String,
    pub tile_size: u32,
    pub width: u32,
}

impl ThumbnailTrack {
    pub fn new(id: String, tile_size: u32, width: u32) -> Self {
        Self {
            id,
            tile_size,
            width,
        }
    }

    /// Resolves this thumbnail configuration against available CMAF tracks.
    pub fn resolve<'a>(
        &self,
        tracks: impl IntoIterator<Item = &'a ResolvedCmafTrack>,
    ) -> Option<ResolvedThumbnailTrack> {
        let source = select_source(self.width / self.tile_size, tracks)?;

        Some(ResolvedThumbnailTrack {
            id: self.id.clone(),
            tile_size: self.tile_size,
            width: self.width,
            source: source.clone(),
        })
    }
}

/// An error encountered while generating thumbnail media.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ThumbnailError(String);

/// A resolved thumbnail track.
#[derive(Clone)]
pub struct ResolvedThumbnailTrack {
    id: String,
    tile_size: u32,
    width: u32,
    source: ResolvedCmafTrack,
}

impl ResolvedThumbnailTrack {
    /// Returns the video track selected to produce this thumbnail sprite.
    pub fn source(&self) -> &ResolvedCmafTrack {
        &self.source
    }

    /// Returns the thumbnail track identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the number of tiles in each sprite row and column.
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// Returns the width of the complete thumbnail sprite.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height of the complete thumbnail sprite.
    pub fn height(&self) -> u32 {
        let CmafKind::Video(video) = self.source.kind() else {
            unreachable!("thumbnail source must be video");
        };
        let height = u64::from(self.width()) * u64::from(video.height) / u64::from(video.width);
        (height - height % u64::from(self.tile_size())) as u32
    }

    /// Returns the dimensions of one thumbnail tile.
    pub fn tile_dimensions(&self) -> (u32, u32) {
        (
            self.width() / self.tile_size,
            self.height() / self.tile_size,
        )
    }

    /// Returns the interval between regular IDR frames, in milliseconds.
    pub fn frame_duration(&self) -> u64 {
        self.source.idr_cadence()
    }

    /// Returns the duration covered by one thumbnail sprite, in milliseconds.
    pub fn sprite_duration(&self) -> u64 {
        u64::from(self.tile_size()).pow(2) * self.frame_duration()
    }

    /// Returns the estimated bandwidth of the JPEG sprite representation.
    pub fn bandwidth(&self) -> u64 {
        (u64::from(self.width()) * u64::from(self.height()) * 1_000)
            .div_ceil(self.sprite_duration())
            .max(1)
    }

    /// Generates thumbnail sprite `number`.
    ///
    /// # Errors
    ///
    /// Returns an error when a frame cannot be read, decoded, composed, or encoded.
    pub async fn jpeg(&self, op: &Operator, number: u32) -> Result<Bytes, ThumbnailError> {
        Sprite::new(op, &self.source, self.tile_dimensions().0, self.tile_size())
            .jpeg(number)
            .await
            .map_err(|error| ThumbnailError(error.to_string()))
    }
}

fn select_source<'a>(
    width: u32,
    tracks: impl IntoIterator<Item = &'a ResolvedCmafTrack>,
) -> Option<&'a ResolvedCmafTrack> {
    let mut smallest_suitable = None;
    let mut largest = None;

    for track in tracks {
        let CmafKind::Video(video) = track.kind() else {
            continue;
        };
        let video_width = video.width;
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
        ThumbnailTrack::new("thumbnail".to_string(), tile_size, width)
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
        let configured = ThumbnailTrack::new("thumbnail".to_string(), 0, 0);
        let source = video("720", 1_280, 720);

        let thumbnail = configured.resolve([&source]).unwrap();

        assert_eq!(thumbnail.source().id(), "720");
    }
}
