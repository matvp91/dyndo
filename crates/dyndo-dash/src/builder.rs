use std::ops::Range;
use std::time::Duration;

use crate::DashError;
use crate::adaptation_group::AdaptationGroup;
use crate::options::DashOptions;
use crate::roles;
use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, S, SegmentTemplate,
    SegmentTimeline, SupplementalProperty,
};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::served_segment::ServedSegment;
use dyndo_core::track::Track;
use dyndo_core::track_kind::TrackKind;

const DASH_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const DASH_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const AUDIO_CHANNEL_CONFIGURATION_SCHEME: &str =
    "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const INITIALIZATION_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const PERIOD_CONTINUITY_SCHEME: &str = "urn:mpeg:dash:period-continuity:2015";

pub(crate) fn build_mpd(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let presentation_duration = presentation_duration(tracks);
    let period_spans = period_spans(
        if dash_options.multi_period {
            &segment_options.boundaries
        } else {
            &[]
        },
        presentation_duration,
    );
    let groups = AdaptationGroup::group(tracks);
    ensure_segment_alignment(&groups, segment_options)?;
    let periods = build_periods(&period_spans, &groups, segment_options);

    Ok(MPD {
        xmlns: Some(DASH_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(DASH_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(u64::from(max_segment_duration(
            tracks,
            segment_options,
        )))),
        mediaPresentationDuration: Some(Duration::from_millis(u64::from(presentation_duration))),
        periods,
        ..Default::default()
    })
}

fn ensure_segment_alignment(
    groups: &[AdaptationGroup<'_>],
    segment_options: &SegmentOptions,
) -> Result<(), DashError> {
    if groups
        .iter()
        .all(|group| group.is_segment_aligned(segment_options))
    {
        Ok(())
    } else {
        Err(DashError::SegmentAlignment)
    }
}

fn build_periods(
    spans: &[Range<u32>],
    groups: &[AdaptationGroup<'_>],
    segment_options: &SegmentOptions,
) -> Vec<Period> {
    let mut periods = Vec::new();
    for span in spans {
        let next = build_period(periods.len(), span, periods.last(), groups, segment_options);
        periods.extend(next);
    }

    periods
}

fn build_period(
    index: usize,
    span: &Range<u32>,
    previous: Option<&Period>,
    groups: &[AdaptationGroup<'_>],
    segment_options: &SegmentOptions,
) -> Option<Period> {
    let adaptations: Vec<AdaptationSet> = groups
        .iter()
        .enumerate()
        .filter_map(|(id, group)| build_adaptation_set(id, group, segment_options, previous, span))
        .collect();
    if adaptations.is_empty() {
        return None;
    }

    Some(Period {
        id: Some(index.to_string()),
        start: Some(Duration::from_millis(u64::from(span.start))),
        duration: Some(Duration::from_millis(u64::from(span.end - span.start))),
        adaptations,
        ..Default::default()
    })
}

fn build_adaptation_set(
    id: usize,
    group: &AdaptationGroup<'_>,
    segment_options: &SegmentOptions,
    previous: Option<&Period>,
    span: &Range<u32>,
) -> Option<AdaptationSet> {
    let representations: Vec<Representation> = group
        .members()
        .iter()
        .filter_map(|track| build_representation(track, segment_options, span))
        .collect();
    if representations.is_empty() {
        return None;
    }

    Some(AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(group.content_type().to_string()),
        mimeType: Some(group.mime_type().to_string()),
        lang: group.language().map(str::to_string),
        segmentAlignment: Some(true),
        startWithSAP: Some(1),
        Role: roles::roles(group.content_type(), group.role()),
        Accessibility: roles::accessibility(group.content_type(), group.role()),
        supplemental_property: build_period_continuity(id, previous),
        representations,
        ..Default::default()
    })
}

// Continuity lets clients keep their decoder across Periods.
fn build_period_continuity(id: usize, previous: Option<&Period>) -> Vec<SupplementalProperty> {
    let id = id.to_string();

    previous
        .filter(|period| {
            period
                .adaptations
                .iter()
                .any(|adaptation_set| adaptation_set.id.as_deref() == Some(id.as_str()))
        })
        .map(|period| SupplementalProperty {
            schemeIdUri: PERIOD_CONTINUITY_SCHEME.to_string(),
            value: period.id.clone(),
            ..Default::default()
        })
        .into_iter()
        .collect()
}

fn build_representation(
    track: &Track,
    segment_options: &SegmentOptions,
    span: &Range<u32>,
) -> Option<Representation> {
    let all_segments = served_segments(track, segment_options);
    let bandwidth = ServedSegment::maximum_bitrate(&all_segments);
    let segments: Vec<_> = all_segments
        .into_iter()
        .filter(|segment| {
            u64::from(span.start) <= segment.start_time()
                && segment.start_time() < u64::from(span.end)
        })
        .collect();
    if segments.is_empty() {
        return None;
    }

    let mut representation = Representation {
        id: Some(track.id().to_string()),
        bandwidth: Some(bandwidth),
        codecs: Some(track.codec().rfc6381()),
        SegmentTemplate: Some(build_segment_template(track, &segments, span)),
        ..Default::default()
    };

    match track.kind() {
        TrackKind::Video(video) => {
            representation.width = Some(u64::from(video.width));
            representation.height = Some(u64::from(video.height));
            representation.frameRate = Some(video.frame_rate.clone());
        }
        TrackKind::Audio(audio) => {
            representation.audioSamplingRate = Some(audio.sample_rate.to_string());
            representation.AudioChannelConfiguration =
                vec![build_audio_channel_configuration(audio.channels)];
        }
        TrackKind::Text(_) => {}
    }

    Some(representation)
}

fn build_audio_channel_configuration(channels: u16) -> AudioChannelConfiguration {
    AudioChannelConfiguration {
        schemeIdUri: AUDIO_CHANNEL_CONFIGURATION_SCHEME.to_string(),
        value: Some(channels.to_string()),
        ..Default::default()
    }
}

fn build_segment_template(
    track: &Track,
    segments: &[ServedSegment<'_>],
    span: &Range<u32>,
) -> SegmentTemplate {
    SegmentTemplate {
        timescale: Some(u64::from(track.timescale())),
        presentationTimeOffset: Some(presentation_time_offset(track, span)),
        initialization: Some(INITIALIZATION_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(build_segment_timeline(segments)),
        ..Default::default()
    }
}

fn presentation_time_offset(track: &Track, span_ms: &Range<u32>) -> u64 {
    // Round down so the offset cannot place the period's segments early.
    let offset = u128::from(span_ms.start) * u128::from(track.timescale()) / 1000;

    track
        .unscaled_earliest_presentation_time()
        .unwrap_or(0)
        .saturating_add(u64::try_from(offset).unwrap_or(u64::MAX))
}

fn build_segment_timeline(segments: &[ServedSegment<'_>]) -> SegmentTimeline {
    let mut entries: Vec<S> = Vec::new();
    let mut end = None;

    for segment in segments {
        let start = segment.unscaled_start_time();
        let segment_end = segment.unscaled_end_time();
        let duration = segment.unscaled_duration();
        let continues = end == Some(start);
        match entries.last_mut() {
            Some(previous) if previous.d == duration && continues => {
                *previous.r.get_or_insert(0) += 1;
            }
            _ => entries.push(S {
                // A missing `t` continues the preceding run, so a run after a gap states it.
                t: (!continues).then_some(start),
                d: duration,
                ..Default::default()
            }),
        }
        end = Some(segment_end);
    }

    SegmentTimeline { segments: entries }
}

fn served_segments<'a>(track: &'a Track, options: &SegmentOptions) -> Vec<ServedSegment<'a>> {
    ServedSegment::group(track.segments(), options.min_length, &options.boundaries)
}

fn presentation_duration(tracks: &[Track]) -> u32 {
    maximum_duration(tracks, |kind| matches!(kind, TrackKind::Video(_))).unwrap_or_else(|| {
        maximum_duration(tracks, |kind| matches!(kind, TrackKind::Audio(_))).unwrap_or(0)
    })
}

fn maximum_duration(tracks: &[Track], include: impl Fn(&TrackKind) -> bool) -> Option<u32> {
    tracks
        .iter()
        .filter(|track| include(track.kind()))
        .map(Track::duration)
        .max()
}

fn max_segment_duration(tracks: &[Track], options: &SegmentOptions) -> u32 {
    tracks
        .iter()
        .filter(|track| matches!(track.kind(), TrackKind::Video(_) | TrackKind::Audio(_)))
        .flat_map(|track| {
            served_segments(track, options).into_iter().map(|segment| {
                let duration = u128::from(segment.unscaled_duration()) * 1_000;
                let duration = duration.div_ceil(u128::from(track.timescale()));
                u32::try_from(duration).unwrap_or(u32::MAX)
            })
        })
        .max()
        .unwrap_or(0)
}

fn period_spans(boundaries: &[u32], duration: u32) -> Vec<Range<u32>> {
    let mut edges: Vec<_> = boundaries
        .iter()
        .copied()
        .filter(|boundary| 0 < *boundary && *boundary < duration)
        .collect();
    edges.sort_unstable();
    edges.dedup();

    let mut spans = Vec::with_capacity(edges.len() + 1);
    let mut start = 0;
    for end in edges.into_iter().chain([duration]) {
        spans.push(start..end);
        start = end;
    }
    spans
}
