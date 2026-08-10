use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt, stream};
use opendal::Operator;

use super::sprite_canvas::SpriteCanvas;
use super::{FrameGrab, FrameGrabError};
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

/// Generates thumbnail sprites with a fixed grid size and sampling step.
#[derive(Clone, Copy)]
pub struct Thumbnail {
    tile_size: u32,
    step: u32,
}

impl Thumbnail {
    pub fn new(tile_size: u32, step: u32) -> Self {
        Self { tile_size, step }
    }

    /// Generates a thumbnail sprite beginning at `time`.
    ///
    /// Returns `None` when thumbnails are disabled or unavailable for the track.
    ///
    /// # Errors
    ///
    /// Returns an error when a frame cannot be read, decoded, composed, or encoded.
    pub async fn generate(
        self,
        op: &Operator,
        track: &Track,
        time: u64,
    ) -> Result<Option<Bytes>, ThumbnailError> {
        if self.tile_size == 0 || self.step == 0 {
            return Ok(None);
        }
        let TrackKind::Video(video) = track.kind() else {
            return Ok(None);
        };
        let width = video.width - video.width % self.tile_size;
        let height = video.height - video.height % self.tile_size;
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let (Some(first), Some(last)) = (track.segments().first(), track.segments().last()) else {
            return Ok(None);
        };
        let Some(start) = first.start_time().checked_add(time) else {
            return Ok(None);
        };
        let end = last.end_time();
        if start >= end {
            return Ok(None);
        }

        let frame_grab = FrameGrab::new(op, track)?;
        let mut canvas = SpriteCanvas::new(self.tile_size, width, height);
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

    fn frame_times(self, start: u64, end: u64) -> impl Iterator<Item = u64> {
        let step = u64::from(self.step);
        (0..u64::from(self.tile_size).pow(2))
            .map_while(move |index| start.checked_add(index * step).filter(|time| *time < end))
    }
}
