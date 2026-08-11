use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt, stream};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ImageFormat, RgbImage, imageops};
use opendal::Operator;

use super::{FrameExtractor, FrameExtractorError};
use crate::thumbnail_descriptor::ThumbnailDescriptor;
use crate::track::Track;
use crate::track_kind::TrackKind;

const CONCURRENT_FRAME_GRABS: usize = 4;
const BITS_PER_PIXEL: u64 = 1;

/// An error encountered while generating a video thumbnail sprite.
#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error(transparent)]
    FrameExtractor(#[from] FrameExtractorError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

/// Generates thumbnail sprites from the most suitable video source.
pub struct Thumbnail<'a> {
    descriptor: &'a ThumbnailDescriptor,
    source: &'a Track,
    height: u32,
}

impl<'a> Thumbnail<'a> {
    /// Creates a thumbnail generator from its configuration and source tracks.
    ///
    /// Selects the smallest video at least as wide as the requested sprite, or
    /// the largest video when every source must be upscaled.
    pub fn new(descriptor: &'a ThumbnailDescriptor, tracks: &'a [Track]) -> Option<Self> {
        let source = select_source(descriptor.width, tracks)?;
        let (_, height) = dimensions(descriptor, source)?;

        Some(Self {
            descriptor,
            source,
            height,
        })
    }

    /// Returns the video track selected to produce this thumbnail sprite.
    pub fn source(&self) -> &Track {
        self.source
    }

    /// Returns the thumbnail configuration identifier.
    pub fn id(&self) -> &str {
        &self.descriptor.id
    }

    /// Returns the number of tiles in each sprite row and column.
    pub fn tile_size(&self) -> u32 {
        self.descriptor.tile_size
    }

    /// Returns the interval between adjacent thumbnail frames, in milliseconds.
    pub fn step(&self) -> u32 {
        self.descriptor.step
    }

    /// Returns the width of the complete thumbnail sprite.
    pub fn width(&self) -> u32 {
        self.descriptor.width
    }

    /// Returns the height of the complete thumbnail sprite.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the dimensions of one thumbnail tile.
    pub fn tile_dimensions(&self) -> (u32, u32) {
        (
            self.width() / self.descriptor.tile_size,
            self.height / self.descriptor.tile_size,
        )
    }

    /// Returns the duration covered by one thumbnail sprite, in milliseconds.
    pub fn sprite_duration(&self) -> u64 {
        u64::from(self.descriptor.tile_size)
            .saturating_mul(u64::from(self.descriptor.tile_size))
            .saturating_mul(u64::from(self.descriptor.step))
    }

    /// Returns the estimated bandwidth of the JPEG sprite representation.
    pub fn bandwidth(&self) -> u64 {
        let bits = u128::from(self.width())
            .saturating_mul(u128::from(self.height))
            .saturating_mul(u128::from(BITS_PER_PIXEL));
        let bits_per_second = bits
            .saturating_mul(1_000)
            .div_ceil(u128::from(self.sprite_duration()));

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

        let extractor = FrameExtractor::new(op, self.source);
        let (tile_width, tile_height) = self.tile_dimensions();
        let mut sprite = RgbImage::new(self.width(), self.height);
        let frames = self
            .frame_times(start, end)
            .map(|time| extractor.jpeg(time, tile_width, tile_height));
        let mut frames = stream::iter(frames).buffered(CONCURRENT_FRAME_GRABS);
        let mut index = 0_u64;
        while let Some(jpeg) = frames.try_next().await? {
            let tile = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg)?
                .resize_exact(tile_width, tile_height, FilterType::Triangle)
                .to_rgb8();
            let column = index % u64::from(self.descriptor.tile_size);
            let row = index / u64::from(self.descriptor.tile_size);
            imageops::replace(
                &mut sprite,
                &tile,
                i64::try_from(column * u64::from(tile_width)).unwrap_or(i64::MAX),
                i64::try_from(row * u64::from(tile_height)).unwrap_or(i64::MAX),
            );
            index += 1;
        }

        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg).encode_image(&sprite)?;
        Ok(Some(Bytes::from(jpeg)))
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
