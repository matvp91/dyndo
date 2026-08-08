use std::ops::Range;

use dash_mpd::{AdaptationSet, EssentialProperty, Representation, SegmentTemplate};
use dyndo_core::asset_descriptor::{TrackKind, VideoKind};
use dyndo_core::segment::Segment;
use dyndo_core::track::Track;

use crate::builder::segment_timeline;
use crate::options::DashOptions;

/// The timescale sprite times are counted in. A sprite is addressed by the presentation
/// time it shows, which every option here already states in milliseconds.
const TIMESCALE: u32 = 1000;

const CONTENT_TYPE: &str = "image";
const MIME_TYPE: &str = "image/jpeg";
const REPRESENTATION_ID: &str = "thumbnails";

/// How a player is told the sprite is divided, as the DASH-IF `NxN` value.
const TILE_SCHEME: &str = "http://dashif.org/guidelines/thumbnail_tile";

/// What a sprite of thumbnails encodes to at quality 80. A cell is a heavy downscale of
/// its frame, so a sprite carries far less detail than a photograph of its size — the
/// bandwidth a client reads off this is an estimate for a resource that does not exist
/// until it is asked for.
const BITS_PER_PIXEL: u64 = 1;

/// The thumbnail track a manifest describes: sprites of `tile_size`×`tile_size`
/// thumbnails, stepping on `step` milliseconds from one thumbnail to the next.
///
/// A sprite comes out the size of the video track it is cut from, so nothing here
/// states its pixels.
pub(crate) struct Thumbnail {
    tile_size: u32,
    step: u32,
}

impl Thumbnail {
    /// The thumbnail track `options` asks for, or `None` when it asks for none.
    ///
    /// The tile size is the only field that says whether a track was asked for at all,
    /// and this is where that is read — nothing downstream sees a sprite of no cells.
    pub(crate) fn new(options: &DashOptions) -> Option<Self> {
        (options.thumbnail_tile_size > 0).then_some(Self {
            tile_size: options.thumbnail_tile_size,
            step: options.thumbnail_step,
        })
    }

    /// Milliseconds one sprite covers, which every sprite of the track covers equally.
    pub(crate) fn duration(&self) -> u64 {
        u64::from(self.tile_size) * u64::from(self.tile_size) * u64::from(self.step)
    }

    /// The sprites of `span_ms`, on a grid that runs from the start of the presentation
    /// however the presentation is divided.
    ///
    /// Every span holds the sprites it overlaps rather than those beginning inside it,
    /// so a period shorter than a sprite still describes the one covering it. A sprite
    /// crossing a boundary is described by both the periods it reaches, at the time it
    /// always shows — the same resource under the same name, which a client already
    /// holding it does not fetch twice.
    pub(crate) fn sprites(&self, span_ms: &Range<u32>) -> Vec<Segment> {
        let duration = self.duration();
        let first = u64::from(span_ms.start) / duration;
        let last = u64::from(span_ms.end).div_ceil(duration);

        // A sprite is cut when it is asked for, so it has no bytes to point at.
        (first..last)
            .map(|index| Segment::new(0, 0, index * duration, duration))
            .collect()
    }

    /// The AdaptationSet describing the sprites of `span_ms`, or `None` when `tracks`
    /// holds no video track to cut them from.
    ///
    /// The sprites come from the first video track and are addressed under its name, so
    /// the manifest's choice of source is the one served rather than one a server has to
    /// arrive at again.
    pub(crate) fn adaptation_set(
        &self,
        id: usize,
        tracks: &[Track],
        span_ms: &Range<u32>,
    ) -> Option<AdaptationSet> {
        let (source, video) = tracks.iter().find_map(|track| match track.kind() {
            TrackKind::Video(video) => Some((track, video)),
            _ => None,
        })?;

        Some(AdaptationSet {
            id: Some(id.to_string()),
            contentType: Some(CONTENT_TYPE.to_string()),
            mimeType: Some(MIME_TYPE.to_string()),
            representations: vec![self.representation(source, video, span_ms)],
            ..Default::default()
        })
    }

    fn representation(
        &self,
        source: &Track,
        video: &VideoKind,
        span_ms: &Range<u32>,
    ) -> Representation {
        Representation {
            id: Some(REPRESENTATION_ID.to_string()),
            bandwidth: Some(self.bandwidth(video)),
            width: Some(u64::from(video.width)),
            height: Some(u64::from(video.height)),
            essential_property: vec![EssentialProperty {
                schemeIdUri: TILE_SCHEME.to_string(),
                value: Some(format!("{0}x{0}", self.tile_size)),
                ..Default::default()
            }],
            SegmentTemplate: Some(SegmentTemplate {
                timescale: Some(u64::from(TIMESCALE)),
                presentationTimeOffset: Some(u64::from(span_ms.start)),
                // The source track names itself, so a sprite is asked for from the
                // track the manifest measured. A sprite has no initialization: it is a
                // whole image rather than a fragment of a stream.
                media: Some(format!("{}/$Time$.jpg", source.id())),
                SegmentTimeline: Some(segment_timeline(&self.sprites(span_ms))),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// What a client should expect a sprite to cost, in bits per second.
    fn bandwidth(&self, video: &VideoKind) -> u64 {
        let bits = u64::from(video.width) * u64::from(video.height) * BITS_PER_PIXEL;

        (bits * u64::from(TIMESCALE)).div_ceil(self.duration())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tiles_is_no_thumbnail_track() {
        assert!(Thumbnail::new(&DashOptions::default()).is_none());
    }

    #[test]
    fn a_sprite_covers_a_step_per_cell() {
        let thumbnail = Thumbnail::new(&options(5)).unwrap();

        assert_eq!(thumbnail.duration(), 250_000);
    }

    /// The last sprite runs past the end of the presentation, which is where the cells
    /// it never reaches go unfilled.
    #[test]
    fn sprites_tile_the_presentation_from_its_start() {
        let thumbnail = Thumbnail::new(&options(5)).unwrap();

        assert_eq!(starts(&thumbnail, &(0..600_000)), vec![0, 250_000, 500_000]);
    }

    #[test]
    fn a_period_shorter_than_a_sprite_still_describes_the_one_covering_it() {
        let thumbnail = Thumbnail::new(&options(5)).unwrap();

        assert_eq!(starts(&thumbnail, &(60_000..120_000)), vec![0]);
    }

    #[test]
    fn a_sprite_crossing_a_boundary_belongs_to_both_periods() {
        let thumbnail = Thumbnail::new(&options(5)).unwrap();

        assert_eq!(
            (
                starts(&thumbnail, &(0..300_000)),
                starts(&thumbnail, &(300_000..600_000))
            ),
            (vec![0, 250_000], vec![250_000, 500_000])
        );
    }

    fn starts(thumbnail: &Thumbnail, span_ms: &Range<u32>) -> Vec<u64> {
        thumbnail
            .sprites(span_ms)
            .iter()
            .map(|sprite| sprite.raw_range().start)
            .collect()
    }

    fn options(tile_size: u32) -> DashOptions {
        DashOptions {
            thumbnail_tile_size: tile_size,
            ..Default::default()
        }
    }
}
