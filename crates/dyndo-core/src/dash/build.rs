use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, Period, Representation, Role, SegmentTemplate,
    SegmentTimeline, MPD, S,
};

use crate::asset::{AudioTrack, Segment, TextTrack, Track, VideoTrack};

const INIT_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const MPD_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const MPD_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const AUDIO_CHANNEL_CONFIG_SCHEME: &str = "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";
const ROLE_SCHEME: &str = "urn:mpeg:dash:role:2011";

fn frame_rate_str(fr: (u32, u32)) -> String {
    if fr.1 == 1 {
        fr.0.to_string()
    } else {
        format!("{}/{}", fr.0, fr.1)
    }
}

fn build_timeline(segments: &[Segment], first_t: u64) -> Vec<S> {
    let mut out: Vec<S> = Vec::new();
    let mut first = true;
    for seg in segments {
        match out.last_mut() {
            Some(last) if last.d == seg.duration => {
                *last.r.get_or_insert(0) += 1;
            }
            _ => {
                out.push(S {
                    t: if first { Some(first_t) } else { None },
                    d: seg.duration,
                    r: None,
                    ..Default::default()
                });
                first = false;
            }
        }
    }
    out
}

fn segment_template(timescale: u32, pto: u64, segments: &[Segment]) -> SegmentTemplate {
    SegmentTemplate {
        timescale: Some(timescale as u64),
        presentationTimeOffset: Some(pto),
        initialization: Some(INIT_TEMPLATE.to_string()),
        media: Some(MEDIA_TEMPLATE.to_string()),
        SegmentTimeline: Some(SegmentTimeline {
            segments: build_timeline(segments, pto),
        }),
        ..Default::default()
    }
}

/// The fields every representation shares: id, bandwidth, codecs, and the
/// segment template. Per-media-type builders add their own dimensions or audio
/// configuration.
fn base_representation(track: &impl Track, codecs: String) -> Representation {
    Representation {
        id: Some(track.id()),
        bandwidth: Some(track.bandwidth() as u64),
        codecs: Some(codecs),
        SegmentTemplate: Some(segment_template(
            track.timescale(),
            track.earliest_presentation_time(),
            track.segments(),
        )),
        ..Default::default()
    }
}

fn video_representation(track: &VideoTrack) -> Representation {
    Representation {
        width: Some(track.width() as u64),
        height: Some(track.height() as u64),
        frameRate: (track.frame_rate().0 != 0).then(|| frame_rate_str(track.frame_rate())),
        ..base_representation(track, track.codec().rfc6381())
    }
}

fn audio_representation(track: &AudioTrack) -> Representation {
    Representation {
        audioSamplingRate: Some(track.sample_rate().to_string()),
        AudioChannelConfiguration: vec![AudioChannelConfiguration {
            schemeIdUri: AUDIO_CHANNEL_CONFIG_SCHEME.to_string(),
            value: Some(track.channels().to_string()),
            ..Default::default()
        }],
        ..base_representation(track, track.codec().rfc6381())
    }
}

/// A text `Representation` carries no dimensions or channel configuration; the
/// shared base (id, bandwidth, `codecs`, segment template) is all it needs.
fn text_representation(track: &TextTrack) -> Representation {
    base_representation(track, track.codec().rfc6381())
}

/// The DASH `Role` that marks a text `AdaptationSet` as translated-dialogue
/// subtitles (as opposed to SDH captions).
fn subtitle_role() -> Vec<Role> {
    vec![Role {
        schemeIdUri: ROLE_SCHEME.to_string(),
        value: Some("subtitle".to_string()),
        ..Default::default()
    }]
}

/// Group track indices by a key, preserving first-seen order of keys and members.
fn group_by_key<T, K: PartialEq>(items: &[T], key: impl Fn(&T) -> K) -> Vec<(K, Vec<usize>)> {
    let mut groups: Vec<(K, Vec<usize>)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let k = key(item);
        match groups.iter_mut().find(|(existing, _)| *existing == k) {
            Some((_, idxs)) => idxs.push(i),
            None => groups.push((k, vec![i])),
        }
    }
    groups
}

/// Build one `AdaptationSet` from a group of like representations. `roles` are
/// the DASH `Role` descriptors for the set (e.g. a `subtitle` role for text);
/// video and audio sets pass none.
fn adaptation_set(
    id: usize,
    content_type: &str,
    mime: &str,
    lang: Option<String>,
    roles: Vec<Role>,
    representations: Vec<Representation>,
) -> AdaptationSet {
    AdaptationSet {
        id: Some(id.to_string()),
        contentType: Some(content_type.to_string()),
        mimeType: Some(mime.to_string()),
        lang,
        segmentAlignment: Some(true),
        startWithSAP: Some(1),
        Role: roles,
        representations,
        ..Default::default()
    }
}

/// Build the raw static VOD `MPD`: one `AdaptationSet` per video codec fourcc,
/// then one per audio `(fourcc, language)`, each track becoming a `Representation`.
fn mpd(videos: &[VideoTrack], audios: &[AudioTrack], texts: &[TextTrack]) -> MPD {
    let mut adaptations = Vec::new();
    let mut set_id = 0;

    for (_fourcc, idxs) in group_by_key(videos, |t| t.codec().fourcc()) {
        let representations = idxs
            .iter()
            .map(|&i| video_representation(&videos[i]))
            .collect();
        adaptations.push(adaptation_set(
            set_id,
            "video",
            "video/mp4",
            None,
            Vec::new(),
            representations,
        ));
        set_id += 1;
    }
    for ((_fourcc, lang), idxs) in
        group_by_key(audios, |t| (t.codec().fourcc(), t.language().to_string()))
    {
        let representations = idxs
            .iter()
            .map(|&i| audio_representation(&audios[i]))
            .collect();
        adaptations.push(adaptation_set(
            set_id,
            "audio",
            "audio/mp4",
            Some(lang),
            Vec::new(),
            representations,
        ));
        set_id += 1;
    }
    for ((_fourcc, lang), idxs) in
        group_by_key(texts, |t| (t.codec().fourcc(), t.language().to_string()))
    {
        let representations = idxs
            .iter()
            .map(|&i| text_representation(&texts[i]))
            .collect();
        adaptations.push(adaptation_set(
            set_id,
            "text",
            "application/mp4",
            Some(lang),
            subtitle_role(),
            representations,
        ));
        set_id += 1;
    }

    let period = Period {
        id: Some("0".to_string()),
        start: Some(Duration::ZERO),
        adaptations,
        ..Default::default()
    };

    let min_buffer_ms = videos
        .iter()
        .map(|t| t.max_segment_duration_ms())
        .chain(audios.iter().map(|t| t.max_segment_duration_ms()))
        .chain(texts.iter().map(|t| t.max_segment_duration_ms()))
        .max()
        .unwrap_or(0);
    let media_duration_ms = videos
        .iter()
        .map(|t| t.duration_ms())
        .chain(audios.iter().map(|t| t.duration_ms()))
        .chain(texts.iter().map(|t| t.duration_ms()))
        .max()
        .unwrap_or(0);

    MPD {
        xmlns: Some(MPD_XMLNS.to_string()),
        mpdtype: Some("static".to_string()),
        profiles: Some(MPD_PROFILE.to_string()),
        minBufferTime: Some(Duration::from_millis(min_buffer_ms)),
        mediaPresentationDuration: Some(Duration::from_millis(media_duration_ms)),
        periods: vec![period],
        ..Default::default()
    }
}

/// Assemble the final `MPD`, optionally hoisting shared `SegmentTemplate` content
/// up to the `AdaptationSet` level.
pub(crate) fn build_mpd(
    videos: &[VideoTrack],
    audios: &[AudioTrack],
    texts: &[TextTrack],
    compact: bool,
) -> MPD {
    let mut m = mpd(videos, audios, texts);
    if compact {
        super::compact::compact(&mut m);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{CmafHeader, TextCmafMetadata};
    use crate::codec::TextCodec;

    fn seg(duration: u64) -> Segment {
        Segment {
            offset: 0,
            size: 0,
            duration,
            duration_ms: duration,
        }
    }

    fn text_track(language: &str) -> TextTrack {
        TextTrack::new(
            String::new(),
            CmafHeader {
                timescale: 1000,
                duration: 4000,
                bandwidth: 256,
                earliest_presentation_time: 0,
                init_segment: seg(0),
                segments: vec![seg(4000)],
            },
            TextCmafMetadata {
                codec: TextCodec::Wvtt,
                language: Some(language.to_string()),
            },
            None,
        )
    }

    #[test]
    fn mpd_advertises_text_adaptation_set_with_subtitle_role() {
        let m = mpd(&[], &[], &[text_track("eng")]);
        let text = m.periods[0]
            .adaptations
            .iter()
            .find(|a| a.contentType.as_deref() == Some("text"))
            .expect("a text AdaptationSet");

        assert_eq!(text.mimeType.as_deref(), Some("application/mp4"));
        assert_eq!(text.lang.as_deref(), Some("eng"));
        assert_eq!(text.Role.len(), 1);
        assert_eq!(text.Role[0].schemeIdUri, ROLE_SCHEME);
        assert_eq!(text.Role[0].value.as_deref(), Some("subtitle"));

        let rep = &text.representations[0];
        assert_eq!(rep.id.as_deref(), Some("text_wvtt_eng"));
        assert_eq!(rep.codecs.as_deref(), Some("wvtt"));
    }

    #[test]
    fn mpd_emits_one_text_adaptation_set_per_language() {
        let m = mpd(&[], &[], &[text_track("eng"), text_track("nld")]);
        let text_sets = m.periods[0]
            .adaptations
            .iter()
            .filter(|a| a.contentType.as_deref() == Some("text"))
            .count();
        assert_eq!(text_sets, 2);
    }

    #[test]
    fn build_timeline_sets_time_only_on_the_first_entry() {
        let out = build_timeline(&[seg(10), seg(20)], 100);
        assert_eq!(out[0].t, Some(100));
        assert_eq!(out[1].t, None);
    }

    #[test]
    fn build_timeline_run_length_encodes_equal_durations() {
        // DASH S@r counts *additional* repeats, so three equal segments
        // collapse to a single S with r = 2.
        let out = build_timeline(&[seg(10), seg(10), seg(10)], 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].r, Some(2));
    }

    #[test]
    fn build_timeline_starts_a_new_entry_on_duration_change() {
        let out = build_timeline(&[seg(10), seg(20)], 0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].d, 20);
    }

    #[test]
    fn frame_rate_str_omits_a_denominator_of_one() {
        assert_eq!(frame_rate_str((30, 1)), "30");
    }

    #[test]
    fn frame_rate_str_keeps_a_fractional_rate() {
        assert_eq!(frame_rate_str((30_000, 1_001)), "30000/1001");
    }

    #[test]
    fn group_by_key_preserves_first_seen_key_order() {
        let items = ["a", "b", "a", "c"];
        let groups = group_by_key(&items, |s| s.to_string());
        let keys: Vec<_> = groups.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn group_by_key_collects_indices_of_like_keys() {
        let items = ["a", "b", "a"];
        let groups = group_by_key(&items, |s| s.to_string());
        assert_eq!(groups[0].1, vec![0, 2]);
    }
}
