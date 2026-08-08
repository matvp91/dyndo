//! DASH manifest construction from dyndo assets.

use std::ops::Range;
use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, S, SegmentTemplate,
    SegmentTimeline, SupplementalProperty,
};
use dyndo_core::asset_descriptor::{AssetDescriptor, TrackKind};
use dyndo_core::boundary_utils::BoundaryUtils;
use dyndo_core::clock_utils::ClockUtils;
use dyndo_core::filter::{Filter, FilterMatchedNothing};
use dyndo_core::segment::{self, Segment, SegmentOptions, max_bitrate, max_segment_duration};
use dyndo_core::track::{Track, TrackError, max_duration};
use opendal::Operator;

use crate::adaptation_set_group::{self, AdaptationSetGroup};
use crate::compact;
use crate::options::DashOptions;
use crate::roles;

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
    Track(#[from] TrackError),
    #[error("tracks in an adaptation set are not segment-aligned")]
    SegmentAlignment,
    #[error(transparent)]
    Filter(#[from] FilterMatchedNothing),
}

/// Generates a static DASH media presentation description for an asset.
///
/// `filter` narrows which of the asset's tracks the manifest describes; pass `None`
/// to describe all of them.
///
/// # Errors
///
/// Returns a [`DashError`] when a track cannot be probed, the filter matches no
/// track, or tracks grouped into an AdaptationSet are not segment-aligned.
pub async fn generate_mpd(
    op: &Operator,
    asset: &AssetDescriptor,
    dash_options: &DashOptions,
    filter: Option<&Filter>,
) -> Result<MPD, DashError> {
    let tracks = Track::probe_all(op, asset).await?;
    let tracks = match filter {
        Some(filter) => filter.narrow(tracks, &asset.segment_options)?,
        None => tracks,
    };

    build_mpd(&tracks, &asset.segment_options, dash_options)
}

/// Builds the manifest from tracks already probed and already narrowed.
fn build_mpd(
    tracks: &[Track],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let duration = Duration::from_millis(u64::from(max_duration(tracks)));
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
    for span in BoundaryUtils::divide(boundaries, max_duration(tracks)) {
        let next = period(
            periods.len(),
            &span,
            periods.last(),
            &groups,
            segment_options,
        );
        periods.extend(next);
    }

    let mut mpd = MPD {
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
    if dash_options.compact {
        compact::compact(&mut mpd);
    }
    Ok(mpd)
}

/// The period a span covers, holding whatever each track has to give it, or
/// `None` when no track has anything.
///
/// A group's id is its index among all of them and never its position among those
/// that survive: it is the key a client matches a rendition on across periods, so
/// renumbering it would pair unrelated tracks.
fn period(
    index: usize,
    span: &Range<u32>,
    previous: Option<&Period>,
    groups: &[AdaptationSetGroup<'_>],
    segment_options: &SegmentOptions,
) -> Option<Period> {
    let adaptations: Vec<AdaptationSet> = groups
        .iter()
        .enumerate()
        .filter_map(|(id, group)| adaptation_set(id, group, segment_options, previous, span))
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
fn adaptation_set(
    id: usize,
    group: &AdaptationSetGroup<'_>,
    segment_options: &SegmentOptions,
    previous: Option<&Period>,
    span: &Range<u32>,
) -> Option<AdaptationSet> {
    let representations: Vec<Representation> = group
        .members()
        .iter()
        .filter_map(|track| representation(track, segment_options, span))
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
        supplemental_property: period_continuity(id, previous),
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
fn period_continuity(id: usize, previous: Option<&Period>) -> Vec<SupplementalProperty> {
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
fn representation(
    track: &Track,
    segment_options: &SegmentOptions,
    span: &Range<u32>,
) -> Option<Representation> {
    let segments = segment::span(track, segment_options, span);
    if segments.is_empty() {
        return None;
    }

    let mut representation = Representation {
        id: Some(track.id().to_string()),
        bandwidth: Some(max_bitrate(track, segment_options)),
        codecs: Some(track.codec().to_string()),
        SegmentTemplate: Some(segment_template(track, &segments, span)),
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
                vec![audio_channel_configuration(audio.channels)];
        }
        TrackKind::Text(_) => {}
    }

    Some(representation)
}

fn audio_channel_configuration(channels: u16) -> AudioChannelConfiguration {
    AudioChannelConfiguration {
        schemeIdUri: AUDIO_CHANNEL_CONFIGURATION_SCHEME.to_string(),
        value: Some(channels.to_string()),
        ..Default::default()
    }
}

fn segment_template(track: &Track, segments: &[Segment], span: &Range<u32>) -> SegmentTemplate {
    SegmentTemplate {
        timescale: Some(u64::from(track.timescale())),
        presentationTimeOffset: Some(presentation_time_offset(track, span)),
        initialization: Some(INITIALIZATION_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(segment_timeline(segments)),
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
fn presentation_time_offset(track: &Track, span: &Range<u32>) -> u64 {
    track.earliest_presentation_time()
        + ClockUtils::raw_floor(u64::from(span.start), track.timescale())
}

/// The timeline `segments` describe, with equal durations folded into one entry
/// repeated `r` times.
///
/// An entry opens with the time its first segment begins at, so a run only continues
/// while the next segment starts where the previous one ended. Two segments of equal
/// duration with a gap between them are two runs, since a player reads the entries as
/// one unbroken stretch.
fn segment_timeline(segments: &[Segment]) -> SegmentTimeline {
    let mut entries: Vec<S> = Vec::new();
    let mut end = None;

    for segment in segments {
        let range = segment.raw_range();
        let continues = end == Some(range.start);
        match entries.last_mut() {
            Some(previous) if previous.d == segment.raw_duration() && continues => {
                *previous.r.get_or_insert(0) += 1;
            }
            _ => entries.push(S {
                // A player reads an entry with no time of its own as continuing from
                // the one before it, so only a run that follows nothing states where
                // it begins: the first, and any that opens after a gap.
                t: (!continues).then_some(range.start),
                d: segment.raw_duration(),
                ..Default::default()
            }),
        }
        end = Some(range.end);
    }

    SegmentTimeline { segments: entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_mpd_creates_a_static_manifest() {
        let mpd = build_mpd(&[], &SegmentOptions::default(), &DashOptions::default()).unwrap();

        assert_eq!(mpd.mpdtype.as_deref(), Some("static"));
    }

    #[test]
    fn generate_mpd_uses_the_segment_based_profile() {
        let mpd = build_mpd(&[], &SegmentOptions::default(), &DashOptions::default()).unwrap();

        assert_eq!(mpd.profiles.as_deref(), Some(DASH_PROFILE));
    }

    #[test]
    fn build_mpd_opens_no_period_for_an_asset_without_tracks() {
        let mpd = build_mpd(&[], &SegmentOptions::default(), &DashOptions::default()).unwrap();

        assert!(mpd.periods.is_empty());
    }

    fn previous_period(adaptation_set_ids: &[usize]) -> Period {
        Period {
            id: Some("1".to_string()),
            adaptations: adaptation_set_ids
                .iter()
                .map(|id| AdaptationSet {
                    id: Some(id.to_string()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn the_first_period_continues_nothing() {
        assert!(period_continuity(0, None).is_empty());
    }

    #[test]
    fn a_later_period_continues_the_one_before_it() {
        let continuity = period_continuity(2, Some(&previous_period(&[0, 1, 2])));

        assert_eq!(
            (
                continuity[0].schemeIdUri.as_str(),
                continuity[0].value.as_deref()
            ),
            (PERIOD_CONTINUITY_SCHEME, Some("1"))
        );
    }

    #[test]
    fn an_adaptation_set_the_period_before_lacked_continues_nothing() {
        assert!(period_continuity(2, Some(&previous_period(&[0, 1]))).is_empty());
    }

    #[test]
    fn audio_channel_configuration_uses_the_mpeg_scheme() {
        let configuration = audio_channel_configuration(2);

        assert_eq!(
            configuration.schemeIdUri,
            AUDIO_CHANNEL_CONFIGURATION_SCHEME
        );
    }

    #[test]
    fn audio_channel_configuration_uses_the_channel_count() {
        let configuration = audio_channel_configuration(2);

        assert_eq!(configuration.value.as_deref(), Some("2"));
    }
}
