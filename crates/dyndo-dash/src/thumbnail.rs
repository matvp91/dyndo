use bytes::Bytes;
use dash_mpd::{
    AdaptationSet, EssentialProperty, Representation, S, SegmentTemplate, SegmentTimeline,
};
use dyndo_core::image::{FrameGrab, FrameGrabError, Sprite, SpriteError};
use dyndo_core::track::Track;
use dyndo_core::track_kind::{TrackKind, VideoKind};
use futures_util::{StreamExt, TryStreamExt, stream};
use opendal::Operator;

use crate::options::DashOptions;

const TIMESCALE: u64 = 1_000;
const CONTENT_TYPE: &str = "image";
const MIME_TYPE: &str = "image/jpeg";
const REPRESENTATION_ID: &str = "thumbnails";
const TILE_SCHEME: &str = "http://dashif.org/guidelines/thumbnail_tile";
const BITS_PER_PIXEL: u64 = 1;
const CONCURRENT_FRAME_GRABS: usize = 4;

/// An error encountered while describing or generating thumbnails.
#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("thumbnails are disabled")]
    Disabled,
    #[error("thumbnail sprite duration overflows")]
    DurationOverflow,
    #[error("thumbnail tile size {tile_size} leaves no pixels in a {width}x{height} video")]
    TileSizeTooLarge {
        tile_size: u32,
        width: u32,
        height: u32,
    },
    #[error("thumbnail sprite time must be a multiple of the sprite duration")]
    InvalidTime,
    #[error("thumbnail sprite at {0} ms is outside the video track")]
    NotFound(u64),
    #[error("thumbnails can only be generated from video tracks")]
    NotVideo,
    #[error(transparent)]
    FrameGrab(#[from] FrameGrabError),
    #[error(transparent)]
    Sprite(#[from] SpriteError),
}

#[derive(Clone, Copy)]
pub(crate) struct Thumbnail {
    tile_size: u32,
    step: u32,
    duration: u64,
}

impl Thumbnail {
    pub(crate) fn new(options: &DashOptions) -> Result<Option<Self>, ThumbnailError> {
        if options.thumbnail_tile_size == 0 || options.thumbnail_step == 0 {
            return Ok(None);
        }

        let tile_size = u64::from(options.thumbnail_tile_size);
        let duration = tile_size
            .checked_mul(tile_size)
            .and_then(|tiles| tiles.checked_mul(u64::from(options.thumbnail_step)))
            .ok_or(ThumbnailError::DurationOverflow)?;

        Ok(Some(Self {
            tile_size: options.thumbnail_tile_size,
            step: options.thumbnail_step,
            duration,
        }))
    }

    pub(crate) fn adaptation_set(
        self,
        id: usize,
        tracks: &[Track],
        presentation_duration: u32,
    ) -> Result<Option<AdaptationSet>, ThumbnailError> {
        let Some((source, video)) = tracks.iter().find_map(|track| match track.kind() {
            TrackKind::Video(video) => Some((track, video)),
            _ => None,
        }) else {
            return Ok(None);
        };
        let (width, height) = self.canvas_dimensions(video)?;
        let first_time = 0;
        let last_time =
            self.sprite_start_time(u64::from(presentation_duration).saturating_sub(1))?;
        let repeats = (last_time - first_time) / self.duration;
        let repeats = i64::try_from(repeats).map_err(|_| ThumbnailError::DurationOverflow)?;

        Ok(Some(AdaptationSet {
            id: Some(id.to_string()),
            contentType: Some(CONTENT_TYPE.to_string()),
            mimeType: Some(MIME_TYPE.to_string()),
            representations: vec![Representation {
                id: Some(REPRESENTATION_ID.to_string()),
                bandwidth: Some(self.bandwidth(width, height)),
                width: Some(u64::from(width)),
                height: Some(u64::from(height)),
                essential_property: vec![EssentialProperty {
                    schemeIdUri: TILE_SCHEME.to_string(),
                    value: Some(format!("{0}x{0}", self.tile_size)),
                    ..Default::default()
                }],
                SegmentTemplate: Some(SegmentTemplate {
                    media: Some(format!("{}/$Time$.jpg", source.id())),
                    timescale: Some(TIMESCALE),
                    presentationTimeOffset: Some(0),
                    SegmentTimeline: Some(SegmentTimeline {
                        segments: vec![S {
                            t: Some(first_time),
                            d: self.duration,
                            r: (repeats != 0).then_some(repeats),
                            ..Default::default()
                        }],
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }))
    }

    async fn jpeg(self, op: &Operator, track: &Track, time: u64) -> Result<Bytes, ThumbnailError> {
        let TrackKind::Video(video) = track.kind() else {
            return Err(ThumbnailError::NotVideo);
        };
        let (width, height) = self.canvas_dimensions(video)?;
        if !time.is_multiple_of(self.duration) {
            return Err(ThumbnailError::InvalidTime);
        }
        let Some(first_segment) = track.segments().first() else {
            return Err(ThumbnailError::NotFound(time));
        };
        let Some(last_segment) = track.segments().last() else {
            return Err(ThumbnailError::NotFound(time));
        };
        let Some(first_time) = first_segment.start_time().checked_add(time) else {
            return Err(ThumbnailError::NotFound(time));
        };
        if first_time >= last_segment.end_time() {
            return Err(ThumbnailError::NotFound(time));
        }

        let frame_grab = FrameGrab::new(op, track)?;
        let mut sprite = Sprite::new(self.tile_size, width, height)?;
        let tile_width = width / self.tile_size;
        let tile_height = height / self.tile_size;
        let frame_times = (0..sprite.capacity()).scan(first_time, |time, _| {
            if *time >= last_segment.end_time() {
                return None;
            }

            let current = *time;
            *time = time
                .checked_add(u64::from(self.step))
                .unwrap_or(last_segment.end_time());
            Some(current)
        });
        let mut frames =
            stream::iter(frame_times.map(|time| frame_grab.jpeg(time, tile_width, tile_height)))
                .buffered(CONCURRENT_FRAME_GRABS);
        while let Some(jpeg) = frames.try_next().await? {
            sprite.add(&jpeg)?;
        }

        sprite.jpeg().map_err(Into::into)
    }

    fn canvas_dimensions(self, video: &VideoKind) -> Result<(u32, u32), ThumbnailError> {
        let width = video.width - video.width % self.tile_size;
        let height = video.height - video.height % self.tile_size;
        if width == 0 || height == 0 {
            return Err(ThumbnailError::TileSizeTooLarge {
                tile_size: self.tile_size,
                width: video.width,
                height: video.height,
            });
        }

        Ok((width, height))
    }

    fn sprite_start_time(self, presentation_time: u64) -> Result<u64, ThumbnailError> {
        presentation_time
            .checked_div(self.duration)
            .and_then(|index| index.checked_mul(self.duration))
            .ok_or(ThumbnailError::DurationOverflow)
    }

    fn bandwidth(self, width: u32, height: u32) -> u64 {
        let bits = u128::from(width)
            .saturating_mul(u128::from(height))
            .saturating_mul(u128::from(BITS_PER_PIXEL));
        let bits_per_second = bits
            .saturating_mul(u128::from(TIMESCALE))
            .div_ceil(u128::from(self.duration));

        u64::try_from(bits_per_second).unwrap_or(u64::MAX).max(1)
    }
}

pub(crate) async fn generate_jpeg(
    op: &Operator,
    track: &Track,
    options: &DashOptions,
    time: u64,
) -> Result<Bytes, ThumbnailError> {
    let thumbnail = Thumbnail::new(options)?.ok_or(ThumbnailError::Disabled)?;

    thumbnail.jpeg(op, track, time).await
}

#[cfg(test)]
mod tests {
    use super::Thumbnail;
    use crate::options::DashOptions;

    #[test]
    fn new_returns_none_when_tile_size_is_zero() {
        let options = DashOptions {
            thumbnail_step: 1_000,
            ..DashOptions::default()
        };

        assert!(Thumbnail::new(&options).unwrap().is_none());
    }

    #[test]
    fn new_returns_none_when_step_is_zero() {
        let options = DashOptions {
            thumbnail_tile_size: 2,
            ..DashOptions::default()
        };

        assert!(Thumbnail::new(&options).unwrap().is_none());
    }
}
