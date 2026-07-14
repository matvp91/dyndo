use std::time::Duration;

use dash_mpd::{
    AdaptationSet, AudioChannelConfiguration, Period, Representation, SegmentTemplate,
    SegmentTimeline, MPD, S,
};

use crate::asset::{Segment, Track};
use crate::cmaf::Metadata;

const INIT_TEMPLATE: &str = "$RepresentationID$/init.mp4";
const MEDIA_TEMPLATE: &str = "$RepresentationID$/$Time$.m4s";
const MPD_XMLNS: &str = "urn:mpeg:dash:schema:mpd:2011";
const MPD_PROFILE: &str = "urn:mpeg:dash:profile:isoff-live:2011";
const AUDIO_CHANNEL_CONFIG_SCHEME: &str = "urn:mpeg:dash:23003:3:audio_channel_configuration:2011";

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

fn representation(track: &Track) -> Representation {
    let h = &track.header;
    let base = Representation {
        id: Some(track.id()),
        bandwidth: Some(h.bandwidth as u64),
        codecs: Some(track.metadata.rfc6381()),
        SegmentTemplate: Some(segment_template(
            h.timescale,
            h.earliest_presentation_time,
            &track.segments,
        )),
        ..Default::default()
    };
    match &track.metadata {
        Metadata::Video(v) => Representation {
            width: Some(v.width as u64),
            height: Some(v.height as u64),
            frameRate: (v.frame_rate.0 != 0).then(|| frame_rate_str(v.frame_rate)),
            ..base
        },
        Metadata::Audio(a) => Representation {
            audioSamplingRate: Some(a.sample_rate.to_string()),
            AudioChannelConfiguration: vec![AudioChannelConfiguration {
                schemeIdUri: AUDIO_CHANNEL_CONFIG_SCHEME.to_string(),
                value: Some(a.channels.to_string()),
                ..Default::default()
            }],
            ..base
        },
    }
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

/// Build the raw static VOD `MPD`. Tracks are grouped into one `AdaptationSet`
/// per `(fourcc, language)` key, each track becoming one `Representation`.
fn mpd(tracks: &[Track]) -> MPD {
    let groups = group_by_key(tracks, |t| {
        (
            t.metadata.fourcc(),
            t.metadata.language().map(str::to_string),
        )
    });

    let adaptations = groups
        .iter()
        .enumerate()
        .map(|(set_id, ((_fourcc, lang), idxs))| {
            let representations = idxs.iter().map(|&i| representation(&tracks[i])).collect();
            // Every track in a group shares a fourcc, so a representative track
            // determines the set's content and mime type.
            let metadata = &tracks[idxs[0]].metadata;
            let content_type = match metadata {
                Metadata::Video(_) => "video",
                Metadata::Audio(_) => "audio",
            };
            AdaptationSet {
                id: Some(set_id.to_string()),
                contentType: Some(content_type.to_string()),
                mimeType: Some(metadata.mime_type().to_string()),
                lang: lang.clone(),
                segmentAlignment: Some(true),
                startWithSAP: Some(1),
                representations,
                ..Default::default()
            }
        })
        .collect();

    let period = Period {
        id: Some("0".to_string()),
        start: Some(Duration::ZERO),
        adaptations,
        ..Default::default()
    };

    let min_buffer_ms = tracks
        .iter()
        .map(Track::max_segment_duration_ms)
        .max()
        .unwrap_or(0);
    let media_duration_ms = tracks.iter().map(Track::duration_ms).max().unwrap_or(0);

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
pub(crate) fn build_mpd(tracks: &[Track], compact: bool) -> MPD {
    let mut m = mpd(tracks);
    if compact {
        super::compact::compact(&mut m);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(duration: u64) -> Segment {
        Segment {
            offset: 0,
            size: 0,
            duration,
        }
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
