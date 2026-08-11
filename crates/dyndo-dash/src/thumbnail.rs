use dash_mpd::{
    AdaptationSet, EssentialProperty, Representation, S, SegmentTemplate, SegmentTimeline,
};
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;

const TIMESCALE: u64 = 1_000;
const CONTENT_TYPE: &str = "image";
const MIME_TYPE: &str = "image/jpeg";
const REPRESENTATION_ID: &str = "image_thumbnails";
const TILE_SCHEME: &str = "http://dashif.org/guidelines/thumbnail_tile";
const BITS_PER_PIXEL: u64 = 1;

pub(crate) fn build_adaptation_set(
    id: usize,
    tracks: &[Track],
    presentation_duration: u32,
    tile_size: u32,
    step: u32,
) -> Option<AdaptationSet> {
    if tile_size == 0 || step == 0 {
        return None;
    }
    let (source, video) = tracks.iter().find_map(|track| match track.kind() {
        TrackKind::Video(video) => Some((track, video)),
        _ => None,
    })?;
    let width = video.width - video.width % tile_size;
    let height = video.height - video.height % tile_size;
    if width == 0 || height == 0 {
        return None;
    }
    let duration = u64::from(tile_size)
        .checked_mul(u64::from(tile_size))?
        .checked_mul(u64::from(step))?;
    let last_time = sprite_start_time(u64::from(presentation_duration).saturating_sub(1), duration);
    let repeats = i64::try_from(last_time / duration).unwrap_or(i64::MAX);

    Some(AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(CONTENT_TYPE.to_string()),
        mimeType: Some(MIME_TYPE.to_string()),
        representations: vec![Representation {
            id: Some(REPRESENTATION_ID.to_string()),
            bandwidth: Some(bandwidth(width, height, duration)),
            width: Some(u64::from(width)),
            height: Some(u64::from(height)),
            essential_property: vec![EssentialProperty {
                schemeIdUri: TILE_SCHEME.to_string(),
                value: Some(format!("{tile_size}x{tile_size}")),
                ..Default::default()
            }],
            SegmentTemplate: Some(SegmentTemplate {
                media: Some(format!("video_{}/$Time$.jpg", source.id())),
                timescale: Some(TIMESCALE),
                presentationTimeOffset: Some(0),
                SegmentTimeline: Some(SegmentTimeline {
                    segments: vec![S {
                        t: Some(0),
                        d: duration,
                        r: (repeats != 0).then_some(repeats),
                        ..Default::default()
                    }],
                }),
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    })
}

fn sprite_start_time(presentation_time: u64, duration: u64) -> u64 {
    presentation_time / duration * duration
}

fn bandwidth(width: u32, height: u32, duration: u64) -> u64 {
    let bits = u128::from(width)
        .saturating_mul(u128::from(height))
        .saturating_mul(u128::from(BITS_PER_PIXEL));
    let bits_per_second = bits
        .saturating_mul(u128::from(TIMESCALE))
        .div_ceil(u128::from(duration));

    u64::try_from(bits_per_second).unwrap_or(u64::MAX).max(1)
}
