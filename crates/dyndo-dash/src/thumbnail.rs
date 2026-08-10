use std::ops::Range;

use bytes::Bytes;
use dash_mpd::{AdaptationSet, EssentialProperty, Representation, SegmentTemplate};
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
    #[error("thumbnail step must be greater than zero")]
    InvalidStep,
    #[error("thumbnail sprite duration overflows")]
    DurationOverflow,
    #[error("thumbnail tile size {tile_size} leaves no pixels in a {width}x{height} video")]
    TileSizeTooLarge {
        tile_size: u32,
        width: u32,
        height: u32,
    },
    #[error("thumbnail sprite number must be greater than zero")]
    InvalidNumber,
    #[error("thumbnail sprite {0} is outside the video track")]
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
        if options.thumbnail_tile_size == 0 {
            return Ok(None);
        }
        if options.thumbnail_step == 0 {
            return Err(ThumbnailError::InvalidStep);
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
        span: &Range<u32>,
    ) -> Result<Option<AdaptationSet>, ThumbnailError> {
        let Some((source, video)) = tracks.iter().find_map(|track| match track.kind() {
            TrackKind::Video(video) => Some((track, video)),
            _ => None,
        }) else {
            return Ok(None);
        };
        let (width, height) = self.canvas_dimensions(video)?;
        let first_number = self.number_at(u64::from(span.start))?;
        let first_time = self.presentation_time(first_number)?;

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
                    media: Some(format!("{}/$Number$.jpg", source.id())),
                    timescale: Some(TIMESCALE),
                    duration: Some(self.duration as f64),
                    startNumber: Some(first_number),
                    // A Period may begin in the middle of a sprite. Retaining its
                    // actual start lets both periods address the same overlapping image.
                    presentationTimeOffset: Some(u64::from(span.start).saturating_sub(first_time)),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }))
    }

    async fn jpeg(
        self,
        op: &Operator,
        track: &Track,
        number: u64,
    ) -> Result<Bytes, ThumbnailError> {
        let TrackKind::Video(video) = track.kind() else {
            return Err(ThumbnailError::NotVideo);
        };
        let (width, height) = self.canvas_dimensions(video)?;
        let presentation_time = self.presentation_time(number)?;
        let Some(first_segment) = track.segments().first() else {
            return Err(ThumbnailError::NotFound(number));
        };
        let Some(last_segment) = track.segments().last() else {
            return Err(ThumbnailError::NotFound(number));
        };
        let Some(first_time) = first_segment.start_time().checked_add(presentation_time) else {
            return Err(ThumbnailError::NotFound(number));
        };
        if first_time >= last_segment.end_time() {
            return Err(ThumbnailError::NotFound(number));
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
        let mut frames = stream::iter(
            frame_times.map(|time| frame_grab.jpeg_scaled(time, tile_width, tile_height)),
        )
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

    fn number_at(self, presentation_time: u64) -> Result<u64, ThumbnailError> {
        presentation_time
            .checked_div(self.duration)
            .and_then(|index| index.checked_add(1))
            .ok_or(ThumbnailError::DurationOverflow)
    }

    fn presentation_time(self, number: u64) -> Result<u64, ThumbnailError> {
        number
            .checked_sub(1)
            .ok_or(ThumbnailError::InvalidNumber)?
            .checked_mul(self.duration)
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
    number: u64,
) -> Result<Bytes, ThumbnailError> {
    let thumbnail = Thumbnail::new(options)?.ok_or(ThumbnailError::Disabled)?;

    thumbnail.jpeg(op, track, number).await
}
