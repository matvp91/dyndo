use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt, stream};
use opendal::Operator;

use super::sprite_canvas::SpriteCanvas;
use super::{FrameGrab, FrameGrabError};
use crate::thumbnail_descriptor::ThumbnailDescriptor;
use crate::track::Track;
use crate::track_kind::TrackKind;

const CONCURRENT_FRAME_GRABS: usize = 4;

/// An error encountered while generating a video thumbnail sprite.
#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error(transparent)]
    FrameGrab(#[from] FrameGrabError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

/// Generates thumbnail sprites from the most suitable video source.
pub struct Thumbnail<'a> {
    descriptor: &'a ThumbnailDescriptor,
    source: &'a Track,
    width: u32,
    height: u32,
}

impl<'a> Thumbnail<'a> {
    /// Creates a thumbnail generator from its configuration and source tracks.
    ///
    /// Selects the smallest video at least as wide as the requested sprite, or
    /// the largest video when every source must be upscaled.
    pub fn new(descriptor: &'a ThumbnailDescriptor, tracks: &'a [Track]) -> Option<Self> {
        let source = select_source(descriptor.width, tracks)?;
        let (width, height) = dimensions(descriptor, source)?;

        Some(Self {
            descriptor,
            source,
            width,
            height,
        })
    }

    /// Returns the video track selected to produce this thumbnail sprite.
    pub fn source(&self) -> &Track {
        self.source
    }

    /// Returns the width of the complete thumbnail sprite.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height of the complete thumbnail sprite.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the duration covered by one thumbnail sprite, in milliseconds.
    pub fn duration(&self) -> u64 {
        u64::from(self.descriptor.tile_size)
            .saturating_mul(u64::from(self.descriptor.tile_size))
            .saturating_mul(u64::from(self.descriptor.step))
    }

    /// Generates a thumbnail sprite beginning at `time`.
    ///
    /// Returns `None` when thumbnails are disabled or unavailable for the track.
    ///
    /// # Errors
    ///
    /// Returns an error when a frame cannot be read, decoded, composed, or encoded.
    pub async fn generate(
        &self,
        op: &Operator,
        time: u64,
    ) -> Result<Option<Bytes>, ThumbnailError> {
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

        let frame_grab = FrameGrab::new(op, self.source)?;
        let mut canvas = SpriteCanvas::new(self.descriptor.tile_size, self.width, self.height);
        let (tile_width, tile_height) = canvas.tile_dimensions();
        let frames = self
            .frame_times(start, end)
            .map(|time| frame_grab.jpeg(time, tile_width, tile_height));
        let mut frames = stream::iter(frames).buffered(CONCURRENT_FRAME_GRABS);
        while let Some(jpeg) = frames.try_next().await? {
            canvas.add(&jpeg)?;
        }

        Ok(Some(canvas.jpeg()?))
    }

    fn frame_times(&self, start: u64, end: u64) -> impl Iterator<Item = u64> {
        let step = u64::from(self.descriptor.step);
        (0..u64::from(self.descriptor.tile_size).pow(2))
            .map_while(move |index| start.checked_add(index * step).filter(|time| *time < end))
    }
}

fn select_source(width: u32, tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .filter_map(video_width)
        .filter(|(_, video_width)| *video_width >= width)
        .min_by_key(|(_, video_width)| *video_width)
        .map(|(track, _)| track)
        .or_else(|| {
            tracks
                .iter()
                .filter_map(video_width)
                .max_by_key(|(_, video_width)| *video_width)
                .map(|(track, _)| track)
        })
}

fn video_width(track: &Track) -> Option<(&Track, u32)> {
    let TrackKind::Video(video) = track.kind() else {
        return None;
    };
    Some((track, video.width))
}

fn dimensions(descriptor: &ThumbnailDescriptor, source: &Track) -> Option<(u32, u32)> {
    let TrackKind::Video(video) = source.kind() else {
        return None;
    };
    if descriptor.tile_size == 0
        || descriptor.width == 0
        || descriptor.step == 0
        || video.width == 0
    {
        return None;
    }
    if !descriptor.width.is_multiple_of(descriptor.tile_size) {
        return None;
    }
    let height = u64::from(descriptor.width).saturating_mul(u64::from(video.height))
        / u64::from(video.width);
    let height = height - height % u64::from(descriptor.tile_size);
    let height = u32::try_from(height).ok()?;
    (height != 0).then_some((descriptor.width, height))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{Thumbnail, ThumbnailDescriptor};
    use crate::codec::{CodecConfig, WvttCodec};
    use crate::segment::InitSegment;
    use crate::track::Track;
    use crate::track_kind::{TrackKind, VideoKind};

    fn video(id: &str, width: u32, height: u32) -> Track {
        Track::new(
            id.to_string(),
            format!("{id}.mp4").into(),
            TrackKind::Video(VideoKind {
                width,
                height,
                frame_rate: "25/1".to_string(),
            }),
            Arc::new(InitSegment::new(CodecConfig::Wvtt(WvttCodec), 1_000, 0, 0)),
            Vec::new(),
        )
    }

    fn descriptor(width: u32) -> ThumbnailDescriptor {
        ThumbnailDescriptor {
            id: "thumbnail".to_string(),
            tile_size: 4,
            width,
            step: 1_000,
        }
    }

    #[test]
    fn new_selects_the_smallest_video_that_meets_the_sprite_width() {
        let tracks = [video("720", 1_280, 720), video("1080", 1_920, 1_080)];
        let descriptor = descriptor(1_500);

        let thumbnail = Thumbnail::new(&descriptor, &tracks).unwrap();

        assert_eq!(thumbnail.source().id(), "1080");
    }

    #[test]
    fn new_uses_the_largest_video_when_all_sources_are_too_small() {
        let tracks = [video("720", 1_280, 720), video("1080", 1_920, 1_080)];
        let descriptor = descriptor(3_840);

        let thumbnail = Thumbnail::new(&descriptor, &tracks).unwrap();

        assert_eq!(thumbnail.source().id(), "1080");
    }
}
