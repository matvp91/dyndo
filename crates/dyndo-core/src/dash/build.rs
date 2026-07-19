use std::time::Duration;

use dash_mpd::{
    Accessibility, AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation, Role, S,
    SegmentTemplate, SegmentTimeline,
};

use super::adaptation_set_group::{self, AdaptationKey};
use crate::asset::Asset;
use crate::metadata::Metadata;
use crate::role::{AudioRole, TextRole};
use crate::segment::Segment;
use crate::track::Track;

const INIT_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const MPD_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const MPD_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const AUDIO_CHANNEL_CONFIG_SCHEME: &str = "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const ROLE_SCHEME: &str = "urn:mpeg:dash:role:2011";
const AUDIO_PURPOSE_SCHEME: &str = "urn:tva:metadata:cs:AudioPurposeCS:2007";

fn build_timeline(segments: &[Segment], first_t: u64) -> Vec<S> {
    let mut out: Vec<S> = Vec::new();
    for seg in segments {
        match out.last_mut() {
            Some(last) if last.d == seg.duration => *last.r.get_or_insert(0) += 1,
            _ => out.push(S {
                d: seg.duration,
                ..Default::default()
            }),
        }
    }
    if let Some(first) = out.first_mut() {
        first.t = Some(first_t);
    }
    out
}

fn segment_template(track: &Track, segments: &[Segment]) -> SegmentTemplate {
    let ept = track.earliest_presentation_time();
    SegmentTemplate {
        timescale: Some(track.timescale() as u64),
        presentationTimeOffset: Some(ept),
        initialization: Some(INIT_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(SegmentTimeline {
            segments: build_timeline(segments, ept),
        }),
        ..Default::default()
    }
}

/// The track's `Representation`: the id, bandwidth, codecs, and segment
/// template every media type shares, plus its type's dimensions or audio
/// configuration. The timeline advertises the served segments under the
/// asset's grouping pair.
fn representation(track: &Track, boundaries_ms: &[u64], min_length_ms: u64) -> Representation {
    let mut rep = Representation {
        id: Some(track.id()),
        bandwidth: Some(track.bandwidth() as u64),
        codecs: track.codec().map(String::from),
        SegmentTemplate: Some(segment_template(
            track,
            &track.segments(boundaries_ms, min_length_ms),
        )),
        ..Default::default()
    };
    match &track.metadata {
        Metadata::Video(v) => {
            rep.width = Some(v.width as u64);
            rep.height = Some(v.height as u64);
            rep.frameRate = match track.frame_rate() {
                (0, _) => None,
                (n, 1) => Some(n.to_string()),
                (n, d) => Some(format!("{n}/{d}")),
            };
        }
        Metadata::Audio(a) => {
            rep.audioSamplingRate = Some(a.sample_rate.to_string());
            rep.AudioChannelConfiguration = vec![AudioChannelConfiguration {
                schemeIdUri: AUDIO_CHANNEL_CONFIG_SCHEME.to_string(),
                value: Some(a.channels.to_string()),
                ..Default::default()
            }];
        }
        Metadata::Text(_) => {}
    }
    rep
}

/// A single `Role` descriptor in the standard role scheme.
fn role_element(value: &str) -> Role {
    Role {
        schemeIdUri: ROLE_SCHEME.to_string(),
        value: Some(value.to_string()),
        ..Default::default()
    }
}

/// The `Role`(s) for a text track. An unset role defaults to `subtitle`,
/// preserving the previous hardcoded behavior.
fn text_roles(role: Option<TextRole>) -> Vec<Role> {
    vec![role_element(role.map_or("subtitle", TextRole::as_str))]
}

/// The `Role`(s) for an audio track. An unset role emits none.
fn audio_roles(role: Option<AudioRole>) -> Vec<Role> {
    role.map(|r| vec![role_element(r.as_str())])
        .unwrap_or_default()
}

/// The `Accessibility` descriptor(s) for an audio track — present only for the
/// audio-description and enhanced-intelligibility roles (AudioPurposeCS).
fn audio_accessibility(role: Option<AudioRole>) -> Vec<Accessibility> {
    let value = match role {
        Some(AudioRole::Description) => "1",
        Some(AudioRole::EnhancedAudioIntelligibility) => "8",
        _ => return Vec::new(),
    };
    vec![Accessibility {
        schemeIdUri: AUDIO_PURPOSE_SCHEME.to_string(),
        value: Some(value.to_string()),
        id: None,
    }]
}

/// The `AdaptationSet` for `key`, with one `Representation` per member.
/// Attributes derive from the key alone, so the emitted set can never
/// disagree with the grouping that formed it.
fn adaptation_set(
    id: usize,
    key: &AdaptationKey,
    members: &[&Track],
    boundaries_ms: &[u64],
    min_length_ms: u64,
) -> AdaptationSet {
    let (content_type, mime, lang, roles, accessibility) = match key {
        AdaptationKey::Video { .. } => ("video", "video/mp4", None, Vec::new(), Vec::new()),
        AdaptationKey::Audio { language, role, .. } => (
            "audio",
            "audio/mp4",
            Some(language.clone()),
            audio_roles(*role),
            audio_accessibility(*role),
        ),
        AdaptationKey::Text { language, role, .. } => (
            "text",
            "application/mp4",
            Some(language.clone()),
            text_roles(*role),
            Vec::new(),
        ),
    };
    AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(content_type.to_string()),
        mimeType: Some(mime.to_string()),
        lang,
        segmentAlignment: Some(true),
        startWithSAP: Some(1),
        Accessibility: accessibility,
        Role: roles,
        representations: members
            .iter()
            .map(|t| representation(t, boundaries_ms, min_length_ms))
            .collect(),
        ..Default::default()
    }
}

/// Build the raw static VOD `MPD`: tracks grouped by their
/// [`AdaptationKey`] — one `AdaptationSet` per distinct key, in first-seen
/// order, each track becoming a `Representation`. The timeline advertises
/// the served segments under the asset's grouping policy; raw (non-CMAF)
/// tracks are not advertised.
fn mpd(asset: &Asset) -> MPD {
    let adaptations = adaptation_set_group::group(&asset.tracks)
        .iter()
        .enumerate()
        .map(|(id, (key, members))| {
            adaptation_set(
                id,
                key,
                members,
                &asset.segment_boundaries_ms,
                asset.min_segment_length_ms,
            )
        })
        .collect();

    let period = Period {
        id: Some("0".to_string()),
        start: Some(Duration::ZERO),
        adaptations,
        ..Default::default()
    };

    MPD {
        xmlns: Some(MPD_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(MPD_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(asset.max_segment_duration_ms())),
        mediaPresentationDuration: Some(Duration::from_millis(asset.duration_ms())),
        periods: vec![period],
        ..Default::default()
    }
}

/// Assemble the final `MPD`, optionally hoisting shared `SegmentTemplate` content
/// up to the `AdaptationSet` level.
pub(super) fn build_mpd(asset: &Asset, compact: bool) -> MPD {
    let mut m = mpd(asset);
    if compact {
        super::compact::compact(&mut m);
    }
    m
}
