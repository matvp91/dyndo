//! DASH manifest construction from dyndo assets.

use std::ops::Range;
use std::time::Duration;

use crate::adaptation_set_group::{self, AdaptationSetGroup};
use crate::options::DashOptions;
use crate::roles;
use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, S, SegmentTemplate,
    SegmentTimeline, SupplementalProperty,
};
use dyndo_core::filter::FilterMatchedNothing;
use dyndo_core::probe::ProbeError;
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

#[derive(Debug, thiserror::Error)]
pub enum DashError {
    #[error(transparent)]
    Probe(#[from] ProbeError),
    #[error("tracks in an adaptation set are not segment-aligned")]
    SegmentAlignment,
    #[error(transparent)]
    Filter(#[from] FilterMatchedNothing),
}

/// Builds the manifest from tracks already probed and already narrowed.
pub(crate) fn build_mpd(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let presentation_duration = presentation_duration(tracks);
    let duration = Duration::from_millis(u64::from(presentation_duration));
    let groups = adaptation_set_group::group(tracks);
    if groups
        .iter()
        .any(|group| !group.is_segment_aligned(segment_options))
    {
        return Err(DashError::SegmentAlignment);
    }
    let boundaries: &[u32] = if dash_options.multi_period {
        &segment_options.boundaries
    } else {
        &[]
    };
    let mut periods: Vec<Period> = Vec::new();
    for span in period_spans(boundaries, presentation_duration) {
        let next = build_period(
            periods.len(),
            &span,
            periods.last(),
            &groups,
            segment_options,
        );
        periods.extend(next);
    }

    let mpd = MPD {
        xmlns: Some(DASH_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(DASH_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(u64::from(max_segment_duration(
            tracks,
            segment_options,
        )))),
        mediaPresentationDuration: Some(duration),
        periods,
        ..Default::default()
    };
    Ok(mpd)
}

/// The period a span covers, holding whatever each track has to give it, or
/// `None` when no track has anything.
///
/// A group's id is its index among all of them and never its position among those
/// that survive: it is the key a client matches a rendition on across periods, so
/// renumbering it would pair unrelated tracks.
fn build_period(
    index: usize,
    span: &Range<u32>,
    previous: Option<&Period>,
    groups: &[AdaptationSetGroup<'_>],
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

/// The AdaptationSet for `group` within `span`, or `None` when none of its
/// renditions reach that far.
fn build_adaptation_set(
    id: usize,
    group: &AdaptationSetGroup<'_>,
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

/// Declares that the AdaptationSet carries on from the one holding the same id in
/// the period before it.
///
/// dyndo only ever cuts a single encode into periods, so every period after the
/// first continues the one before on an unbroken timeline. Left unsaid, a client
/// is entitled to tear down its decoder at each period it crosses. A group the
/// previous period never carried says nothing, since it begins here rather than
/// continuing.
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

/// The Representation for `track` within `span`, or `None` when the track has no
/// segments there — a timeline has to hold at least one, and there would be
/// nothing to fetch anyway.
fn build_representation(
    track: &Track,
    segment_options: &SegmentOptions,
    span: &Range<u32>,
) -> Option<Representation> {
    let all_segments = served_segments(track, segment_options);
    let bandwidth = ServedSegment::maximum_bitrate(&all_segments);
    let segments: Vec<_> = all_segments
        .into_iter()
        .filter(|segment| span_contains(span, segment.start_time()))
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

/// The media time the period begins at, which is what the times in its timeline
/// are read against.
///
/// This is the period's own start rather than the track's first segment, so that
/// a track cutting after the boundary presents where it always did instead of
/// being pulled back to the period edge. The difference between the two shows up
/// as the gap between this and the first time in the timeline.
fn presentation_time_offset(track: &Track, span_ms: &Range<u32>) -> u64 {
    // The span is milliseconds and the timeline counts the track's timescale units.
    // Rounded down, so the offset never lands past the segment the period opens on —
    // a player reading the timeline against it would place every segment early.
    let offset = u128::from(span_ms.start) * u128::from(track.timescale()) / 1000;

    track
        .unscaled_earliest_presentation_time()
        .unwrap_or(0)
        .saturating_add(u64::try_from(offset).unwrap_or(u64::MAX))
}

/// The timeline `segments` describe, with equal durations folded into one entry
/// repeated `r` times.
///
/// An entry opens with the time its first segment begins at, so a run only continues
/// while the next segment starts where the previous one ended. Two segments of equal
/// duration with a gap between them are two runs, since a player reads the entries as
/// one unbroken stretch.
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
                // A player reads an entry with no time of its own as continuing from
                // the one before it, so only a run that follows nothing states where
                // it begins: the first, and any that opens after a gap.
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

fn span_contains(span: &Range<u32>, time: u64) -> bool {
    u64::from(span.start) <= time && time < u64::from(span.end)
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

/// Divides the presentation at valid unique boundaries.
///
/// Boundaries at or beyond either edge produce no empty Period because an empty
/// Period cannot carry a representation timeline.
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
