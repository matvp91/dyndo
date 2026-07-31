use std::collections::HashSet;
use std::time::Duration;

use dash_mpd::{
    Accessibility, AdaptationSet, AudioChannelConfiguration, MPD, Period, Representation,
    Role as DashRole, S, SegmentTemplate, SegmentTimeline,
};
use futures_util::future::try_join_all;
use opendal::Operator;

use super::adaptation_set_group::{self, AdaptationKey};
use crate::next::asset::Asset;
use crate::next::error::Error;
use crate::next::format::Format;
use crate::next::group_segments::group_segments;
use crate::next::role::Role;
use crate::next::segment_index::SegmentIndex;
use crate::next::track::Track;
use crate::next::track_metadata::Kind;

const INIT_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const MPD_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const MPD_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const AUDIO_CHANNEL_CONFIG_SCHEME: &str = "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const ROLE_SCHEME: &str = "urn:mpeg:dash:role:2011";
const AUDIO_PURPOSE_SCHEME: &str = "urn:tva:metadata:cs:AudioPurposeCS:2007";

async fn prepare_track<'a>(
    op: &Operator,
    track: &'a Track,
    minimum_segment_length_ms: u64,
    segment_boundaries_ms: &[u64],
) -> Result<Option<(&'a Track, SegmentIndex)>, Error> {
    if Format::from_path(&track.path)? != Format::Cmaf {
        return Ok(None);
    }
    let index = SegmentIndex::read(op, &track.path).await?;
    let index = group_segments(&index, minimum_segment_length_ms, segment_boundaries_ms)?;
    Ok(Some((track, index)))
}

fn timeline(index: &SegmentIndex) -> Vec<S> {
    let mut timeline: Vec<S> = Vec::new();
    for segment in &index.segments {
        match timeline.last_mut() {
            Some(last) if last.d == segment.duration => *last.r.get_or_insert(0) += 1,
            _ => timeline.push(S {
                t: Some(segment.start),
                d: segment.duration,
                ..Default::default()
            }),
        }
    }
    timeline
}

fn segment_template(index: &SegmentIndex) -> SegmentTemplate {
    SegmentTemplate {
        timescale: Some(u64::from(index.timescale)),
        presentationTimeOffset: Some(index.presentation_time_offset()),
        initialization: Some(INIT_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(SegmentTimeline {
            segments: timeline(index),
        }),
        ..Default::default()
    }
}

fn representation(track: &Track, index: &SegmentIndex) -> Representation {
    let mut representation = Representation {
        id: Some(track.id.clone()),
        bandwidth: Some(index.bandwidth),
        codecs: track.metadata.codec.clone(),
        SegmentTemplate: Some(segment_template(index)),
        ..Default::default()
    };

    match &track.metadata.kind {
        Kind::Video(video) => {
            representation.width = Some(u64::from(video.width));
            representation.height = Some(u64::from(video.height));
            representation.frameRate = Some(frame_rate(video.frame_rate));
        }
        Kind::Audio(audio) => {
            representation.audioSamplingRate = Some(audio.sample_rate.to_string());
            representation.AudioChannelConfiguration = vec![AudioChannelConfiguration {
                schemeIdUri: AUDIO_CHANNEL_CONFIG_SCHEME.to_string(),
                value: Some(audio.channels.to_string()),
                ..Default::default()
            }];
        }
        Kind::Text(_) => {}
    }
    representation
}

fn frame_rate(frame_rate: f64) -> String {
    let denominator = 1_000_000u64;
    let numerator = (frame_rate * denominator as f64).round() as u64;
    let divisor = gcd(numerator, denominator);
    let numerator = numerator / divisor;
    let denominator = denominator / divisor;
    if denominator == 1 {
        numerator.to_string()
    } else {
        format!("{numerator}/{denominator}")
    }
}

fn gcd(left: u64, right: u64) -> u64 {
    if right == 0 {
        left
    } else {
        gcd(right, left % right)
    }
}

fn roles(role: Option<Role>) -> Vec<DashRole> {
    role.map(|role| DashRole {
        schemeIdUri: ROLE_SCHEME.to_string(),
        value: Some(role.as_str().to_string()),
        ..Default::default()
    })
    .into_iter()
    .collect()
}

fn accessibility(role: Option<Role>) -> Vec<Accessibility> {
    let value = match role {
        Some(Role::Description) => "1",
        Some(Role::EnhancedAudioIntelligibility) => "8",
        _ => return Vec::new(),
    };
    vec![Accessibility {
        schemeIdUri: AUDIO_PURPOSE_SCHEME.to_string(),
        value: Some(value.to_string()),
        id: None,
    }]
}

fn adaptation_set(
    id: usize,
    key: &AdaptationKey,
    members: &[&(&Track, SegmentIndex)],
) -> AdaptationSet {
    let (content_type, mime_type, language, role, accessibility) = match key {
        AdaptationKey::Video { role, .. } => ("video", "video/mp4", None, *role, Vec::new()),
        AdaptationKey::Audio { language, role, .. } => (
            "audio",
            "audio/mp4",
            Some(language.clone()),
            *role,
            accessibility(*role),
        ),
        AdaptationKey::Text { language, role, .. } => (
            "text",
            "application/mp4",
            Some(language.clone()),
            *role,
            Vec::new(),
        ),
    };

    AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(content_type.to_string()),
        mimeType: Some(mime_type.to_string()),
        lang: language,
        Accessibility: accessibility,
        Role: roles(role),
        representations: members
            .iter()
            .map(|(track, index)| representation(track, index))
            .collect(),
        ..Default::default()
    }
}

pub(super) async fn build_mpd(op: &Operator, asset: &Asset, compact: bool) -> Result<MPD, Error> {
    let tracks = try_join_all(asset.tracks.iter().map(|track| {
        prepare_track(
            op,
            track,
            asset.min_segment_length_ms,
            &asset.segment_boundaries_ms,
        )
    }))
    .await?
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    let mut ids = HashSet::new();
    for (track, _) in &tracks {
        if !ids.insert(track.id.as_str()) {
            return Err(Error::SerializeDash(format!(
                "duplicate Representation id `{}`",
                track.id
            )));
        }
    }

    let adaptations = adaptation_set_group::group(&tracks)
        .iter()
        .enumerate()
        .map(|(id, (key, members))| adaptation_set(id, key, members))
        .collect();

    let duration_ms = tracks
        .iter()
        .map(|(_, index)| index.duration_ms())
        .max()
        .unwrap_or(0);
    let max_segment_ms = tracks
        .iter()
        .map(|(_, index)| index.max_segment_duration_ms())
        .max()
        .unwrap_or(0);

    let mut mpd = MPD {
        xmlns: Some(MPD_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(MPD_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(max_segment_ms)),
        mediaPresentationDuration: Some(Duration::from_millis(duration_ms)),
        maxSegmentDuration: Some(Duration::from_millis(max_segment_ms)),
        periods: vec![Period {
            id: Some("0".to_string()),
            start: Some(Duration::ZERO),
            duration: Some(Duration::from_millis(duration_ms)),
            adaptations,
            ..Default::default()
        }],
        ..Default::default()
    };
    if compact {
        super::compact::compact(&mut mpd);
    }
    Ok(mpd)
}
