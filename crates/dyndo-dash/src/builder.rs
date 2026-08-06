//! DASH manifest construction from dyndo assets.

use std::ops::Range;
use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, S, SegmentTemplate,
    SegmentTimeline, SupplementalProperty,
};
use dyndo_core::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
use dyndo_core::segment::SegmentOptions;
use dyndo_core::segment_group::{self, SegmentGroup};
use dyndo_core::track::{Track, TrackError, max_bitrate, max_duration, max_segment_duration};
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
}

/// Generates a static DASH media presentation description for an asset.
///
/// # Errors
///
/// Returns a [`DashError`] when a track cannot be probed or tracks grouped into
/// an AdaptationSet are not segment-aligned.
pub async fn generate_mpd(
    op: &Operator,
    asset: &AssetDescriptor,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let tracks = Track::probe_all(op, asset).await?;
    build_mpd(asset, &tracks, &asset.segment_options, dash_options)
}

fn build_mpd(
    asset: &AssetDescriptor,
    tracks: &[Track],
    segment_options: &SegmentOptions,
    dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let duration = Duration::from_millis(u64::from(max_duration(tracks)));
    let groups = adaptation_set_group::group(asset, tracks);
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
    let periods = segment_group::spans(boundaries, max_duration(tracks))
        .iter()
        .enumerate()
        .map(|(index, span)| period(index, span, &groups, segment_options))
        .collect();

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

/// The period a span covers, holding whatever each track has to give it.
fn period(
    index: usize,
    span: &Range<u32>,
    groups: &[AdaptationSetGroup<'_>],
    segment_options: &SegmentOptions,
) -> Period {
    Period {
        id: Some(index.to_string()),
        start: Some(Duration::from_millis(u64::from(span.start))),
        duration: Some(Duration::from_millis(u64::from(span.end - span.start))),
        adaptations: groups
            .iter()
            .enumerate()
            .map(|(id, group)| adaptation_set(id, group, segment_options, index, span))
            .collect(),
        ..Default::default()
    }
}

fn adaptation_set(
    id: usize,
    group: &AdaptationSetGroup<'_>,
    segment_options: &SegmentOptions,
    period: usize,
    span: &Range<u32>,
) -> AdaptationSet {
    AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(group.content_type().to_string()),
        mimeType: Some(group.mime_type().to_string()),
        lang: group.language().map(str::to_string),
        segmentAlignment: Some(true),
        startWithSAP: Some(1),
        Role: roles::roles(group.content_type(), group.role()),
        Accessibility: roles::accessibility(group.content_type(), group.role()),
        supplemental_property: period_continuity(period),
        representations: group
            .members()
            .iter()
            .map(|(descriptor, track)| representation(descriptor, track, segment_options, span))
            .collect(),
        ..Default::default()
    }
}

/// Declares that the AdaptationSet carries on from the one holding the same id in
/// the period before it.
///
/// dyndo only ever cuts a single encode into periods, so every period after the
/// first continues the one before on an unbroken timeline. Left unsaid, a client
/// is entitled to tear down its decoder at each period it crosses.
fn period_continuity(period: usize) -> Vec<SupplementalProperty> {
    if period == 0 {
        return Vec::new();
    }

    vec![SupplementalProperty {
        schemeIdUri: PERIOD_CONTINUITY_SCHEME.to_string(),
        value: Some((period - 1).to_string()),
        ..Default::default()
    }]
}

fn representation(
    descriptor: &TrackDescriptor,
    track: &Track,
    segment_options: &SegmentOptions,
    span: &Range<u32>,
) -> Representation {
    let mut representation = Representation {
        id: Some(descriptor.id.clone()),
        bandwidth: Some(max_bitrate(track, segment_options)),
        codecs: Some(descriptor.codec.clone()),
        SegmentTemplate: Some(segment_template(track, segment_options, span)),
        ..Default::default()
    };

    match &descriptor.kind {
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

    representation
}

fn audio_channel_configuration(channels: u16) -> AudioChannelConfiguration {
    AudioChannelConfiguration {
        schemeIdUri: AUDIO_CHANNEL_CONFIGURATION_SCHEME.to_string(),
        value: Some(channels.to_string()),
        ..Default::default()
    }
}

fn segment_template(
    track: &Track,
    segment_options: &SegmentOptions,
    span: &Range<u32>,
) -> SegmentTemplate {
    let group = segment_group::group_segments(track, segment_options, span);

    SegmentTemplate {
        timescale: Some(u64::from(track.timescale())),
        presentationTimeOffset: Some(presentation_time_offset(track, span)),
        initialization: Some(INITIALIZATION_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(segment_timeline(&group)),
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
    let offset = u128::from(span.start) * u128::from(track.timescale()) / 1000;

    track.earliest_presentation_time()
        + u64::try_from(offset).expect("a period starts within the media timeline")
}

fn segment_timeline(group: &SegmentGroup) -> SegmentTimeline {
    let mut segments: Vec<S> = Vec::new();

    for segment in group.segments() {
        match segments.last_mut() {
            Some(previous) if previous.d == segment.raw_duration() => {
                *previous.r.get_or_insert(0) += 1;
            }
            _ => segments.push(S {
                d: segment.raw_duration(),
                ..Default::default()
            }),
        }
    }

    if let Some(first) = segments.first_mut() {
        first.t = Some(group.start());
    }

    SegmentTimeline { segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> AssetDescriptor {
        AssetDescriptor::default()
    }

    #[test]
    fn generate_mpd_creates_a_static_manifest() {
        let mpd = build_mpd(
            &asset(),
            &[],
            &SegmentOptions::default(),
            &DashOptions::default(),
        )
        .unwrap();

        assert_eq!(mpd.mpdtype.as_deref(), Some("static"));
    }

    #[test]
    fn generate_mpd_uses_the_segment_based_profile() {
        let mpd = build_mpd(
            &asset(),
            &[],
            &SegmentOptions::default(),
            &DashOptions::default(),
        )
        .unwrap();

        assert_eq!(mpd.profiles.as_deref(), Some(DASH_PROFILE));
    }

    #[test]
    fn generate_mpd_creates_one_period() {
        let mpd = build_mpd(
            &asset(),
            &[],
            &SegmentOptions::default(),
            &DashOptions::default(),
        )
        .unwrap();

        assert_eq!(mpd.periods.len(), 1);
    }

    #[test]
    fn the_first_period_continues_nothing() {
        assert!(period_continuity(0).is_empty());
    }

    #[test]
    fn a_later_period_continues_the_one_before_it() {
        let continuity = period_continuity(2);

        assert_eq!(
            (
                continuity[0].schemeIdUri.as_str(),
                continuity[0].value.as_deref()
            ),
            (PERIOD_CONTINUITY_SCHEME, Some("1"))
        );
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
