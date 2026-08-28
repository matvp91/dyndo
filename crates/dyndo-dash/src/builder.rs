use std::{sync::Arc, time::Duration};

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, EssentialProperty, MPD, Period, Representation, S,
    SegmentTemplate, SegmentTimeline,
};
use dyndo_core::{
    asset::Asset,
    codec_config::{CodecConfig, WvttCodec},
    delivery_index::DeliveryIndex,
    media_index::MediaIndex,
    mp4_readable::Mp4Readable,
    segment::Segment,
    segment_index::SegmentIndex,
    text::Subtitle,
    track::{TextTrack, ThumbnailTrack, Track, VideoCmafTrack},
};
use serde::Serialize;

use crate::{DashError, compact, options::DashOptions, roles, split};

const DASH_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const DASH_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const AUDIO_CHANNEL_CONFIGURATION_SCHEME: &str =
    "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const INITIALIZATION_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const THUMBNAIL_MEDIA_TEMPLATE: &str = "$RepresentationID$/$Number$.jpg";
const THUMBNAIL_TILE_SCHEME: &str = "http://dashif.org/guidelines/thumbnail_tile";

pub(crate) async fn generate(asset: &Asset, options: DashOptions) -> Result<String, DashError> {
    if options.text_segment_duration < Duration::from_millis(1) {
        return Err(DashError::TextSegmentDuration);
    }

    let mut adaptations = Vec::new();
    let mut presentation_duration = Duration::ZERO;
    let mut min_buffer_time = Duration::ZERO;

    for track in &asset.tracks {
        if matches!(track, Track::Thumbnail(_)) {
            continue;
        }
        let Some(index) = delivery_index(asset, track, options).await? else {
            continue;
        };
        if index.is_empty() {
            continue;
        }

        let codec = match track {
            Track::Video(track) => &track.codec,
            Track::Audio(track) => &track.codec,
            Track::Text(TextTrack::Cmaf(track)) => &track.codec,
            Track::Text(TextTrack::Sidecar(_)) => &CodecConfig::Wvtt(WvttCodec),
            Track::Thumbnail(_) => continue,
        };
        let representation = build_representation(track, codec, &index);
        add_representation(&mut adaptations, track, representation);

        if matches!(track, Track::Video(_) | Track::Audio(_)) {
            presentation_duration = presentation_duration.max(index_duration(&index));
            min_buffer_time = min_buffer_time.max(max_segment_duration(&index));
        }
    }

    for track in &asset.tracks {
        let Track::Thumbnail(thumbnail) = track else {
            continue;
        };
        if let Some(adaptation) =
            build_thumbnail_adaptation(asset, thumbnail, presentation_duration).await?
        {
            adaptations.push(adaptation);
        }
    }

    let mut mpd = build_mpd(adaptations, presentation_duration, min_buffer_time);
    if options.multi_period {
        mpd = split::split(mpd, &asset.boundaries)?;
    }
    if options.compact {
        mpd = compact::compact(mpd);
    }

    serialize(&mpd)
}

async fn build_thumbnail_adaptation(
    asset: &Asset,
    thumbnail: &ThumbnailTrack,
    presentation_duration: Duration,
) -> Result<Option<AdaptationSet>, DashError> {
    let Some((source_track, source)) = thumbnail_source(asset, thumbnail) else {
        return Ok(None);
    };
    let Some(path) = asset.track_path(source_track) else {
        return Ok(None);
    };
    let index = SegmentIndex::from_path(&path).await?;
    let Some(first) = index.segments().first() else {
        return Ok(None);
    };
    let tile_size = u64::from(thumbnail.metadata.tile_size);
    if tile_size == 0 || thumbnail.metadata.width == 0 || source.metadata.width == 0 {
        return Ok(None);
    }

    let width = u64::from(thumbnail.metadata.width);
    let height = width * u64::from(source.metadata.height) / u64::from(source.metadata.width);
    let height = height - height % tile_size;
    let duration = first.duration_ticks().saturating_mul(tile_size.pow(2));
    if height == 0 || duration == 0 {
        return Ok(None);
    }
    let bandwidth = width
        .saturating_mul(height)
        .saturating_mul(u64::from(index.timescale()))
        .div_ceil(duration);
    let presentation_ticks = duration_ticks(presentation_duration, index.timescale());
    let segment_count = presentation_ticks.div_ceil(duration);

    Ok(Some(AdaptationSet {
        contentType: Some("image".into()),
        mimeType: Some("image/jpeg".into()),
        representations: vec![Representation {
            id: Some(thumbnail.id.clone()),
            bandwidth: Some(bandwidth.max(1)),
            width: Some(width),
            height: Some(height),
            essential_property: vec![EssentialProperty {
                schemeIdUri: THUMBNAIL_TILE_SCHEME.into(),
                value: Some(format!("{tile_size}x{tile_size}")),
                ..Default::default()
            }],
            SegmentTemplate: Some(SegmentTemplate {
                media: Some(THUMBNAIL_MEDIA_TEMPLATE.into()),
                startNumber: Some(1),
                timescale: Some(u64::from(index.timescale())),
                presentationTimeOffset: Some(0),
                SegmentTimeline: Some(SegmentTimeline {
                    segments: vec![S {
                        t: Some(0),
                        d: duration,
                        r: (segment_count > 1)
                            .then(|| i64::try_from(segment_count - 1).unwrap_or(i64::MAX)),
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

fn duration_ticks(duration: Duration, timescale: u32) -> u64 {
    let ticks = duration.as_nanos().saturating_mul(u128::from(timescale)) / 1_000_000_000;

    u64::try_from(ticks).unwrap_or(u64::MAX)
}

fn thumbnail_source<'a>(
    asset: &'a Asset,
    thumbnail: &ThumbnailTrack,
) -> Option<(&'a Track, &'a VideoCmafTrack)> {
    let tile_width = thumbnail.metadata.width / thumbnail.metadata.tile_size.max(1);
    let videos = asset.tracks.iter().filter_map(|track| match track {
        Track::Video(video) => Some((track, video)),
        _ => None,
    });
    let suitable = videos
        .clone()
        .filter(|(_, video)| video.metadata.width >= tile_width)
        .min_by_key(|(_, video)| video.metadata.width);

    suitable.or_else(|| videos.max_by_key(|(_, video)| video.metadata.width))
}

async fn delivery_index(
    asset: &Asset,
    track: &Track,
    options: DashOptions,
) -> Result<Option<DeliveryIndex>, DashError> {
    let Some(path) = asset.track_path(track) else {
        return Ok(None);
    };

    if let Track::Text(TextTrack::Sidecar(_)) = track {
        let subtitle = Subtitle::from_path(&path).await?;
        return Ok(Some(DeliveryIndex::strict(
            subtitle.duration(),
            options.text_segment_duration,
            &asset.boundaries,
        )));
    }

    let source = SegmentIndex::from_path(&path).await?;
    Ok(Some(DeliveryIndex::source_aligned(
        Arc::clone(source.init_segment()),
        source.segments(),
        options.min_segment_duration,
        &asset.boundaries,
    )))
}

fn build_representation(
    track: &Track,
    codec: &CodecConfig,
    index: &DeliveryIndex,
) -> Representation {
    let mut representation = Representation {
        id: Some(track.id()),
        bandwidth: Some(index.max_bitrate()),
        codecs: Some(codec.to_string()),
        SegmentTemplate: Some(SegmentTemplate {
            timescale: Some(u64::from(index.timescale())),
            presentationTimeOffset: index.presentation_time_offset(),
            initialization: Some(INITIALIZATION_TEMPLATE.into()),
            media: Some(MEDIA_TEMPLATE.into()),
            SegmentTimeline: Some(build_segment_timeline(index.segments())),
            ..Default::default()
        }),
        ..Default::default()
    };

    match track {
        Track::Video(track) => {
            representation.width = Some(u64::from(track.metadata.width));
            representation.height = Some(u64::from(track.metadata.height));
            representation.frameRate = Some(track.metadata.frame_rate.to_string());
        }
        Track::Audio(track) => {
            representation.audioSamplingRate = Some(track.metadata.sample_rate.to_string());
            representation.AudioChannelConfiguration = vec![AudioChannelConfiguration {
                schemeIdUri: AUDIO_CHANNEL_CONFIGURATION_SCHEME.into(),
                value: Some(track.metadata.channels.to_string()),
                ..Default::default()
            }];
        }
        Track::Text(_) | Track::Thumbnail(_) => {}
    }
    representation
}

fn add_representation(
    adaptations: &mut Vec<AdaptationSet>,
    track: &Track,
    representation: Representation,
) {
    let candidate = build_adaptation_set(track, representation);
    if let Some(adaptation) = adaptations
        .iter_mut()
        .find(|adaptation| compatible(adaptation, &candidate))
    {
        adaptation.representations.extend(candidate.representations);
        adaptation.segmentAlignment = Some(true);
    } else {
        adaptations.push(candidate);
    }
}

fn build_adaptation_set(track: &Track, representation: Representation) -> AdaptationSet {
    let (content_type, mime_type, language, role, text, audio) = match track {
        Track::Video(_) => ("video", "video/mp4", None, None, false, false),
        Track::Audio(track) => (
            "audio",
            "audio/mp4",
            Some(track.metadata.language.to_string()),
            track.metadata.role,
            false,
            true,
        ),
        Track::Text(track) => {
            let metadata = match track {
                TextTrack::Cmaf(track) => &track.metadata,
                TextTrack::Sidecar(track) => &track.metadata,
            };
            (
                "text",
                "application/mp4",
                Some(metadata.language.to_string()),
                metadata.role,
                true,
                false,
            )
        }
        Track::Thumbnail(_) => unreachable!("thumbnail tracks do not map to CMAF representations"),
    };

    AdaptationSet {
        contentType: Some(content_type.into()),
        mimeType: Some(mime_type.into()),
        lang: language,
        startWithSAP: Some(1),
        Role: roles::role(role, text, audio),
        Accessibility: roles::accessibility(role, text, audio),
        representations: vec![representation],
        ..Default::default()
    }
}

fn compatible(left: &AdaptationSet, right: &AdaptationSet) -> bool {
    left.contentType == right.contentType
        && left.mimeType == right.mimeType
        && left.lang == right.lang
        && left.Role == right.Role
        && left.Accessibility == right.Accessibility
        && representations_compatible(left.representations.first(), right.representations.first())
}

fn representations_compatible(
    left: Option<&Representation>,
    right: Option<&Representation>,
) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    left.codecs == right.codecs
        && left.audioSamplingRate == right.audioSamplingRate
        && left.AudioChannelConfiguration == right.AudioChannelConfiguration
        && segment_templates_compatible(
            left.SegmentTemplate.as_ref(),
            right.SegmentTemplate.as_ref(),
        )
}

fn segment_templates_compatible(
    left: Option<&SegmentTemplate>,
    right: Option<&SegmentTemplate>,
) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };

    left.timescale == right.timescale
        && left.presentationTimeOffset == right.presentationTimeOffset
        && left.SegmentTimeline == right.SegmentTimeline
}

fn build_segment_timeline(segments: &[Segment]) -> SegmentTimeline {
    let mut entries: Vec<S> = Vec::new();
    let mut end = None;
    for segment in segments {
        let start = segment.start_ticks();
        let segment_end = segment.end_ticks();
        let duration = segment.duration_ticks();
        let continues = end == Some(start);
        match entries.last_mut() {
            Some(previous) if previous.d == duration && continues => {
                *previous.r.get_or_insert(0) += 1
            }
            _ => entries.push(S {
                t: (!continues).then_some(start),
                d: duration,
                ..Default::default()
            }),
        }
        end = Some(segment_end);
    }
    SegmentTimeline { segments: entries }
}

fn index_duration(index: &DeliveryIndex) -> Duration {
    let Some((first, remaining)) = index.segments().split_first() else {
        return Duration::ZERO;
    };
    let last = remaining.last().unwrap_or(first);
    last.end_time().saturating_sub(first.start_time())
}

fn max_segment_duration(index: &DeliveryIndex) -> Duration {
    index
        .segments()
        .iter()
        .map(Segment::duration_time)
        .max()
        .unwrap_or(Duration::ZERO)
}

fn build_mpd(
    mut adaptations: Vec<AdaptationSet>,
    presentation_duration: Duration,
    min_buffer_time: Duration,
) -> MPD {
    for (id, adaptation) in adaptations.iter_mut().enumerate() {
        adaptation.id = Some(id.to_string());
    }
    let periods = (!adaptations.is_empty()).then_some(Period {
        id: Some("0".into()),
        start: Some(Duration::ZERO),
        duration: Some(presentation_duration),
        adaptations,
        ..Default::default()
    });
    MPD {
        xmlns: Some(DASH_XMLNS.into()),
        mpdtype: Some("static".into()),
        profiles: Some(DASH_PROFILE.into()),
        minBufferTime: Some(min_buffer_time),
        mediaPresentationDuration: Some(presentation_duration),
        periods: periods.into_iter().collect(),
        ..Default::default()
    }
}

fn serialize(mpd: &MPD) -> Result<String, DashError> {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    mpd.serialize(serializer)
        .map_err(|error| DashError::Serialization(error.to_string()))?;
    Ok(xml)
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use dash_mpd::{Representation, S, SegmentTemplate, SegmentTimeline};
    use dyndo_core::{
        delivery_index::DeliveryIndex,
        segment::{InitSegment, Segment},
    };

    use super::{index_duration, representations_compatible};

    fn representation(codec: &str, start: u64, durations: &[u64]) -> Representation {
        Representation {
            codecs: Some(codec.into()),
            SegmentTemplate: Some(SegmentTemplate {
                timescale: Some(1_000),
                presentationTimeOffset: Some(start),
                SegmentTimeline: Some(SegmentTimeline {
                    segments: durations
                        .iter()
                        .enumerate()
                        .map(|(index, &duration)| S {
                            t: (index == 0).then_some(start),
                            d: duration,
                            ..Default::default()
                        })
                        .collect(),
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn index_duration_excludes_the_presentation_time_offset() {
        let init = Arc::new(InitSegment::new(0..100, 1_000));
        let source = vec![
            Segment::new(Arc::clone(&init), 10_000, 11_000, 100..200),
            Segment::new(Arc::clone(&init), 11_000, 12_000, 200..300),
        ];
        let index = DeliveryIndex::source_aligned(init, &source, Duration::ZERO, &[]);

        assert_eq!(index_duration(&index), Duration::from_secs(2));
    }

    #[test]
    fn representations_are_compatible_when_codec_and_timeline_match() {
        let left = representation("avc1.640028", 0, &[1_000, 1_000]);
        let right = representation("avc1.640028", 0, &[1_000, 1_000]);

        assert!(representations_compatible(Some(&left), Some(&right)));
    }

    #[test]
    fn representations_are_not_compatible_when_segment_boundaries_differ() {
        let left = representation("avc1.640028", 0, &[1_000, 1_000]);
        let right = representation("avc1.640028", 0, &[500, 1_500]);

        assert!(!representations_compatible(Some(&left), Some(&right)));
    }
}
