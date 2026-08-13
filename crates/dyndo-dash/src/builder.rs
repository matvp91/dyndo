use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, CencPssh, ContentProtection, MPD, Period,
    Representation, S, SegmentTemplate, SegmentTimeline,
};
use dyndo_core::drm::Protection;
use dyndo_core::time::Time;
use dyndo_core::track::cmaf::{CmafMetadata, ResolvedCmafTrack, ServedSegment};
use dyndo_core::track::thumbnail::ResolvedThumbnailTrack;

use crate::adaptation_group::AdaptationGroup;
use crate::roles;

const DASH_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const DASH_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const AUDIO_CHANNEL_CONFIGURATION_SCHEME: &str =
    "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const INITIALIZATION_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";

pub(crate) fn build_mpd(
    tracks: &[ResolvedCmafTrack],
    thumbnails: &[ResolvedThumbnailTrack],
    min_length: u32,
    boundaries: &[u32],
) -> MPD {
    let presentation_duration = presentation_duration(tracks);
    let groups = AdaptationGroup::group(tracks);
    let mut adaptations: Vec<AdaptationSet> = groups
        .iter()
        .enumerate()
        .filter_map(|(id, group)| build_adaptation_set(id, group, min_length, boundaries))
        .collect();
    adaptations.extend(crate::thumbnail::build_adaptation_sets(
        groups.len(),
        thumbnails,
        presentation_duration,
    ));
    let periods = (!adaptations.is_empty()).then_some(Period {
        id: Some("0".to_string()),
        start: Some(Duration::ZERO),
        duration: Some(Duration::from_millis(u64::from(presentation_duration))),
        adaptations,
        ..Default::default()
    });

    MPD {
        xmlns: Some(DASH_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(DASH_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(u64::from(max_segment_duration(
            tracks, min_length, boundaries,
        )))),
        mediaPresentationDuration: Some(Duration::from_millis(u64::from(presentation_duration))),
        periods: periods.into_iter().collect(),
        ..Default::default()
    }
}

fn build_adaptation_set(
    id: usize,
    group: &AdaptationGroup<'_>,
    min_length: u32,
    boundaries: &[u32],
) -> Option<AdaptationSet> {
    let representations: Vec<Representation> = group
        .members()
        .iter()
        .filter_map(|track| build_representation(track, min_length, boundaries))
        .collect();
    if representations.is_empty() {
        return None;
    }

    Some(AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(group.track_type().as_str().to_string()),
        mimeType: Some(group.mime_type().to_string()),
        lang: group.language().map(str::to_string),
        startWithSAP: Some(1),
        Role: roles::roles(group.track_type(), group.role()),
        Accessibility: roles::accessibility(group.track_type(), group.role()),
        representations,
        ..Default::default()
    })
}

fn build_representation(
    track: &ResolvedCmafTrack,
    min_length: u32,
    boundaries: &[u32],
) -> Option<Representation> {
    let segments = track.served_segments(min_length, boundaries);
    if segments.is_empty() {
        return None;
    }

    let mut representation = Representation {
        id: Some(track.id().to_string()),
        bandwidth: Some(ServedSegment::maximum_bitrate(&segments)),
        codecs: Some(track.codec().rfc6381()),
        SegmentTemplate: Some(build_segment_template(track, &segments)),
        ..Default::default()
    };

    match track.metadata() {
        CmafMetadata::Video(video) => {
            representation.width = Some(u64::from(video.width));
            representation.height = Some(u64::from(video.height));
            representation.frameRate = Some(video.frame_rate.clone());
        }
        CmafMetadata::Audio(audio) => {
            representation.audioSamplingRate = Some(audio.sample_rate.to_string());
            representation.AudioChannelConfiguration =
                vec![build_audio_channel_configuration(audio.channels)];
        }
        CmafMetadata::Text(_) => {}
    }

    if let Some(protection) = track.protection() {
        representation.ContentProtection = content_protection(protection);
    }
    Some(representation)
}

fn content_protection(protection: &Protection) -> Vec<ContentProtection> {
    let mut entries = vec![ContentProtection {
        schemeIdUri: "urn:mpeg:dash:mp4protection:2011".to_string(),
        value: Some(protection.scheme().as_str().to_string()),
        default_KID: Some(protection.key_id().to_string()),
        ..Default::default()
    }];
    entries.extend(protection.systems().iter().map(|system| ContentProtection {
        schemeIdUri: format!("urn:uuid:{}", system.system_id()),
        cenc_pssh: vec![CencPssh {
            content: Some(BASE64_STANDARD.encode(system.pssh())),
        }],
        ..Default::default()
    }));
    entries
}

fn build_audio_channel_configuration(channels: u16) -> AudioChannelConfiguration {
    AudioChannelConfiguration {
        schemeIdUri: AUDIO_CHANNEL_CONFIGURATION_SCHEME.to_string(),
        value: Some(channels.to_string()),
        ..Default::default()
    }
}

fn build_segment_template(
    track: &ResolvedCmafTrack,
    segments: &[ServedSegment<'_>],
) -> SegmentTemplate {
    SegmentTemplate {
        timescale: Some(u64::from(track.timescale())),
        presentationTimeOffset: track.unscaled_earliest_presentation_time(),
        initialization: Some(INITIALIZATION_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(build_segment_timeline(segments)),
        ..Default::default()
    }
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

fn presentation_duration(tracks: &[ResolvedCmafTrack]) -> u32 {
    maximum_duration(tracks, |metadata| {
        matches!(metadata, CmafMetadata::Video(_))
    })
    .unwrap_or_else(|| {
        maximum_duration(tracks, |metadata| {
            matches!(metadata, CmafMetadata::Audio(_))
        })
        .unwrap_or(0)
    })
}

fn maximum_duration(
    tracks: &[ResolvedCmafTrack],
    include: impl Fn(&CmafMetadata) -> bool,
) -> Option<u32> {
    tracks
        .iter()
        .filter(|track| include(track.metadata()))
        .map(ResolvedCmafTrack::duration)
        .max()
}

fn max_segment_duration(tracks: &[ResolvedCmafTrack], min_length: u32, boundaries: &[u32]) -> u32 {
    tracks
        .iter()
        .filter(|track| {
            matches!(
                track.metadata(),
                CmafMetadata::Video(_) | CmafMetadata::Audio(_)
            )
        })
        .flat_map(|track| {
            track
                .served_segments(min_length, boundaries)
                .into_iter()
                .map(|segment| {
                    u32::try_from(Time::milliseconds_ceil(
                        segment.unscaled_duration(),
                        track.timescale(),
                    ))
                    .unwrap_or(u32::MAX)
                })
        })
        .max()
        .unwrap_or(0)
}
