use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, S, SegmentTemplate,
    SegmentTimeline,
};
use dyndo_core::segment_options::SegmentOptions;
use dyndo_core::track::cmaf::{CmafKind, ResolvedCmafTrack, ServedSegment};
use dyndo_core::track::thumbnail::ResolvedThumbnailTrack;

use crate::DashError;
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
    segment_options: &SegmentOptions,
) -> Result<MPD, DashError> {
    let presentation_duration = presentation_duration(tracks);
    let groups = AdaptationGroup::group(tracks);
    ensure_segment_alignment(&groups, segment_options)?;
    let mut adaptations: Vec<AdaptationSet> = groups
        .iter()
        .enumerate()
        .filter_map(|(id, group)| build_adaptation_set(id, group, segment_options))
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

    Ok(MPD {
        xmlns: Some(DASH_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(DASH_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(u64::from(max_segment_duration(
            tracks,
            segment_options,
        )))),
        mediaPresentationDuration: Some(Duration::from_millis(u64::from(presentation_duration))),
        periods: periods.into_iter().collect(),
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

fn build_adaptation_set(
    id: usize,
    group: &AdaptationGroup<'_>,
    segment_options: &SegmentOptions,
) -> Option<AdaptationSet> {
    let representations: Vec<Representation> = group
        .members()
        .iter()
        .filter_map(|track| build_representation(track, segment_options))
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
        representations,
        ..Default::default()
    })
}

fn build_representation(
    track: &ResolvedCmafTrack,
    segment_options: &SegmentOptions,
) -> Option<Representation> {
    let segments = served_segments(track, segment_options);
    if segments.is_empty() {
        return None;
    }

    let mut representation = Representation {
        id: Some(resource_name(track)),
        bandwidth: Some(ServedSegment::maximum_bitrate(&segments)),
        codecs: Some(track.codec().rfc6381()),
        SegmentTemplate: Some(build_segment_template(track, &segments)),
        ..Default::default()
    };

    match track.kind() {
        CmafKind::Video(video) => {
            representation.width = Some(u64::from(video.width));
            representation.height = Some(u64::from(video.height));
            representation.frameRate = Some(video.frame_rate.clone());
        }
        CmafKind::Audio(audio) => {
            representation.audioSamplingRate = Some(audio.sample_rate.to_string());
            representation.AudioChannelConfiguration =
                vec![build_audio_channel_configuration(audio.channels)];
        }
        CmafKind::Text(_) => {}
    }

    Some(representation)
}

fn resource_name(track: &ResolvedCmafTrack) -> String {
    format!("{}_{}", track.kind().content_type(), track.id())
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

fn served_segments<'a>(
    track: &'a ResolvedCmafTrack,
    options: &SegmentOptions,
) -> Vec<ServedSegment<'a>> {
    ServedSegment::group(track.segments(), options.min_length, &options.boundaries)
}

fn presentation_duration(tracks: &[ResolvedCmafTrack]) -> u32 {
    maximum_duration(tracks, |kind| matches!(kind, CmafKind::Video(_))).unwrap_or_else(|| {
        maximum_duration(tracks, |kind| matches!(kind, CmafKind::Audio(_))).unwrap_or(0)
    })
}

fn maximum_duration(
    tracks: &[ResolvedCmafTrack],
    include: impl Fn(&CmafKind) -> bool,
) -> Option<u32> {
    tracks
        .iter()
        .filter(|track| include(track.kind()))
        .map(ResolvedCmafTrack::duration)
        .max()
}

fn max_segment_duration(tracks: &[ResolvedCmafTrack], options: &SegmentOptions) -> u32 {
    tracks
        .iter()
        .filter(|track| matches!(track.kind(), CmafKind::Video(_) | CmafKind::Audio(_)))
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
