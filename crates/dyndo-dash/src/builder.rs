//! DASH manifest construction from dyndo assets.

use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, S, SegmentTemplate,
    SegmentTimeline,
};
use dyndo_core::asset_descriptor::{AssetDescriptor, TrackDescriptor, TrackKind};
use dyndo_core::segment::SegmentOptions;
use dyndo_core::track::{Track, TrackError};
use dyndo_core::track_helpers::{
    max_bitrate, max_duration_ms, max_segment_duration_ms, read_all_tracks,
};
use opendal::Operator;

use crate::adaptation_set_group::{self, AdaptationSetGroup};
use crate::options::DashOptions;
use crate::roles;

const DASH_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const DASH_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const AUDIO_CHANNEL_CONFIGURATION_SCHEME: &str =
    "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const INITIALIZATION_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";

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
    segment_options: &SegmentOptions,
    _dash_options: &DashOptions,
) -> Result<MPD, DashError> {
    let tracks = read_all_tracks(op, asset).await?;
    build_mpd(asset, &tracks, segment_options)
}

fn build_mpd(
    asset: &AssetDescriptor,
    tracks: &[Track],
    segment_options: &SegmentOptions,
) -> Result<MPD, DashError> {
    let duration = Duration::from_millis(max_duration_ms(tracks));
    let groups = adaptation_set_group::group(asset, tracks);
    if groups.iter().any(|group| {
        !group.is_segment_aligned(
            &asset.segment_boundaries,
            segment_options.min_segment_length_ms,
        )
    }) {
        return Err(DashError::SegmentAlignment);
    }
    let adaptations = groups
        .iter()
        .enumerate()
        .map(|(index, group)| adaptation_set(index, group, asset, segment_options))
        .collect();

    Ok(MPD {
        xmlns: Some(DASH_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(DASH_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(max_segment_duration_ms(
            tracks,
            &asset.segment_boundaries,
            segment_options.min_segment_length_ms,
        ))),
        mediaPresentationDuration: Some(duration),
        periods: vec![Period {
            id: Some("0".to_string()),
            start: Some(Duration::ZERO),
            duration: Some(duration),
            adaptations,
            ..Default::default()
        }],
        ..Default::default()
    })
}

fn adaptation_set(
    index: usize,
    group: &AdaptationSetGroup<'_>,
    asset: &AssetDescriptor,
    segment_options: &SegmentOptions,
) -> AdaptationSet {
    AdaptationSet {
        id: Some(index.to_string()),
        contentType: Some(group.content_type().to_string()),
        mimeType: Some(group.mime_type().to_string()),
        lang: group.language().map(str::to_string),
        segmentAlignment: Some(true),
        startWithSAP: Some(1),
        Role: roles::roles(group.content_type(), group.role()),
        Accessibility: roles::accessibility(group.content_type(), group.role()),
        SegmentTemplate: Some(segment_template(group, asset, segment_options)),
        representations: group
            .members()
            .iter()
            .map(|(descriptor, track)| representation(descriptor, track, asset, segment_options))
            .collect(),
        ..Default::default()
    }
}

fn representation(
    descriptor: &TrackDescriptor,
    track: &Track,
    asset: &AssetDescriptor,
    segment_options: &SegmentOptions,
) -> Representation {
    let mut representation = Representation {
        id: Some(descriptor.id.clone()),
        bandwidth: Some(max_bitrate(
            track,
            &asset.segment_boundaries,
            segment_options.min_segment_length_ms,
        )),
        codecs: Some(descriptor.codec.clone()),
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
    group: &AdaptationSetGroup<'_>,
    asset: &AssetDescriptor,
    segment_options: &SegmentOptions,
) -> SegmentTemplate {
    let track = group.members()[0].1;

    SegmentTemplate {
        timescale: Some(u64::from(track.timescale())),
        presentationTimeOffset: Some(track.earliest_presentation_time()),
        initialization: Some(INITIALIZATION_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(segment_timeline(track, asset, segment_options)),
        ..Default::default()
    }
}

fn segment_timeline(
    track: &Track,
    asset: &AssetDescriptor,
    segment_options: &SegmentOptions,
) -> SegmentTimeline {
    let mut segments: Vec<S> = Vec::new();

    for segment in track.segments(
        &asset.segment_boundaries,
        segment_options.min_segment_length_ms,
    ) {
        match segments.last_mut() {
            Some(previous) if previous.d == segment.duration() => {
                *previous.r.get_or_insert(0) += 1;
            }
            _ => segments.push(S {
                d: segment.duration(),
                ..Default::default()
            }),
        }
    }

    if let Some(first) = segments.first_mut() {
        first.t = Some(track.earliest_presentation_time());
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
        let mpd = build_mpd(&asset(), &[], &SegmentOptions::default()).unwrap();

        assert_eq!(mpd.mpdtype.as_deref(), Some("static"));
    }

    #[test]
    fn generate_mpd_uses_the_segment_based_profile() {
        let mpd = build_mpd(&asset(), &[], &SegmentOptions::default()).unwrap();

        assert_eq!(mpd.profiles.as_deref(), Some(DASH_PROFILE));
    }

    #[test]
    fn generate_mpd_creates_one_period() {
        let mpd = build_mpd(&asset(), &[], &SegmentOptions::default()).unwrap();

        assert_eq!(mpd.periods.len(), 1);
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
