use dash_mpd::{
    AdaptationSet, EssentialProperty, Representation, S, SegmentTemplate, SegmentTimeline,
};
use dyndo_core::track::thumbnail::ResolvedThumbnailTrack;

const TIMESCALE: u64 = 1_000;
const CONTENT_TYPE: &str = "image";
const MIME_TYPE: &str = "image/jpeg";
const TILE_SCHEME: &str = "http://dashif.org/guidelines/thumbnail_tile";

pub(crate) fn build_adaptation_sets(
    id: usize,
    thumbnails: &[ResolvedThumbnailTrack],
    presentation_duration: u32,
) -> Vec<AdaptationSet> {
    thumbnails
        .iter()
        .enumerate()
        .map(|(index, thumbnail)| {
            build_adaptation_set(id + index, thumbnail, presentation_duration)
        })
        .collect()
}

fn build_adaptation_set(
    id: usize,
    thumbnail: &ResolvedThumbnailTrack,
    presentation_duration: u32,
) -> AdaptationSet {
    let duration = thumbnail.sprite_duration();
    let last_time = sprite_start_time(u64::from(presentation_duration).saturating_sub(1), duration);
    let repeats = i64::try_from(last_time / duration).unwrap_or(i64::MAX);

    AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(CONTENT_TYPE.to_string()),
        mimeType: Some(MIME_TYPE.to_string()),
        representations: vec![Representation {
            id: Some(thumbnail.id().to_string()),
            bandwidth: Some(thumbnail.bandwidth()),
            width: Some(u64::from(thumbnail.width())),
            height: Some(u64::from(thumbnail.height())),
            essential_property: vec![EssentialProperty {
                schemeIdUri: TILE_SCHEME.to_string(),
                value: Some(format!(
                    "{}x{}",
                    thumbnail.tile_size(),
                    thumbnail.tile_size()
                )),
                ..Default::default()
            }],
            SegmentTemplate: Some(SegmentTemplate {
                media: Some(format!("{}/$Number$.jpg", thumbnail.id())),
                startNumber: Some(0),
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
    }
}

fn sprite_start_time(presentation_time: u64, duration: u64) -> u64 {
    presentation_time / duration * duration
}
