use std::time::Duration;

use dyndo_core::asset::{Segment, Track};
use hls_m3u8::tags::ExtXMap;
use hls_m3u8::types::PlaylistType;
use hls_m3u8::{MediaPlaylist, MediaSegment};

/// Build the VOD media playlist for `track`: an `EXT-X-MAP` init on the first
/// segment, then one segment per (sub)segment named by its running presentation
/// time. `EXT-X-TARGETDURATION` is the longest segment in whole seconds.
pub(crate) fn build_media(track: &Track) -> MediaPlaylist<'static> {
    let repr = track.id();
    let timescale = track.header.timescale;

    let mut time = track.header.earliest_presentation_time;
    let mut segments: Vec<MediaSegment<'static>> = Vec::with_capacity(track.segments.len());
    for (i, seg) in track.segments.iter().enumerate() {
        let mut b = MediaSegment::builder();
        b.uri(format!("{repr}/{time}.m4s"));
        b.duration(Duration::from_secs_f64(
            seg.duration as f64 / timescale as f64,
        ));
        if i == 0 {
            b.map(ExtXMap::new(format!("{repr}/init.mp4")));
        }
        segments.push(b.build().unwrap());
        time += seg.duration;
    }

    let mut b = MediaPlaylist::builder();
    b.media_sequence(0);
    b.target_duration(Duration::from_secs(target_duration(
        &track.segments,
        timescale,
    )));
    b.playlist_type(PlaylistType::Vod);
    b.has_end_list(true);
    b.segments(segments);
    b.build().unwrap()
}

/// The longest segment in whole seconds, rounded up (RFC 8216 requires an
/// integer `EXT-X-TARGETDURATION` ≥ every segment's rounded duration).
fn target_duration(segments: &[Segment], timescale: u32) -> u64 {
    segments
        .iter()
        .map(|s| (s.duration as f64 / timescale as f64).ceil() as u64)
        .max()
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use dyndo_core::asset::{Segment, Track};
    use dyndo_core::cmaf::{AudioMetadata, Header, Metadata, VideoMetadata};
    use dyndo_core::codec::{AudioCodec, VideoCodec};

    use super::*;

    /// A header with `bandwidth` and one `Segment` per entry in `segs` (each
    /// carrying only a duration; offsets/sizes are irrelevant to playlists).
    fn header(timescale: u32, bandwidth: u32, segs: &[u64]) -> (Header, Vec<Segment>) {
        let segments = segs
            .iter()
            .map(|&d| Segment {
                offset: 0,
                size: 0,
                duration: d,
            })
            .collect();
        let h = Header {
            timescale,
            duration: segs.iter().sum(),
            bandwidth,
            earliest_presentation_time: 0,
            init_segment: Segment {
                offset: 0,
                size: 0,
                duration: 0,
            },
        };
        (h, segments)
    }

    fn video_track(height: u32, bandwidth: u32, segs: &[u64]) -> Track {
        let (h, segments) = header(90_000, bandwidth, segs);
        Track {
            path: String::new(),
            header: h,
            metadata: Metadata::Video(VideoMetadata {
                codec: VideoCodec::Avc {
                    profile: 0x64,
                    constraints: 0x00,
                    level: 0x28,
                },
                width: height * 16 / 9,
                height,
                frame_rate: (25, 1),
            }),
            segments,
        }
    }

    // Unused by this task's test; kept verbatim for Task 2, which reuses it.
    #[allow(dead_code)]
    fn audio_track(
        codec: AudioCodec,
        lang: &str,
        channels: u16,
        bandwidth: u32,
        segs: &[u64],
    ) -> Track {
        let (h, segments) = header(48_000, bandwidth, segs);
        Track {
            path: String::new(),
            header: h,
            metadata: Metadata::Audio(AudioMetadata {
                codec,
                sample_rate: 48_000,
                channels,
                language: lang.to_string(),
            }),
            segments,
        }
    }

    #[test]
    fn media_playlist_has_vod_map_and_running_time_segments() {
        // 90_000 timescale; segments 2s, 2s, 1s → presentation times 0, 180000, 360000.
        let track = video_track(720, 128_000, &[180_000, 180_000, 90_000]);
        let repr = track.id(); // video_avc1_720_128000
        let m = build_media(&track).to_string();

        assert!(m.contains("#EXT-X-PLAYLIST-TYPE:VOD"), "{m}");
        assert!(m.contains("#EXT-X-TARGETDURATION:2"), "{m}");
        assert_eq!(m.matches("#EXT-X-MAP").count(), 1, "{m}");
        assert!(
            m.contains(&format!("#EXT-X-MAP:URI=\"{repr}/init.mp4\"")),
            "{m}"
        );
        assert!(m.contains(&format!("{repr}/0.m4s")), "{m}");
        assert!(m.contains(&format!("{repr}/180000.m4s")), "{m}");
        assert!(m.contains(&format!("{repr}/360000.m4s")), "{m}");
        // 180_000 units @ 90_000 = 2s → EXTINF derives duration from the
        // timescale, so this catches a timescale/duration miscalculation that
        // a URI-only check would miss.
        assert!(m.contains("#EXTINF:2,"), "{m}");
        assert!(m.contains("#EXTINF:1,"), "{m}");
        assert!(m.contains("#EXT-X-ENDLIST"), "{m}");
    }

    #[test]
    fn media_segment_uris_reflect_nonzero_presentation_time() {
        // Nonzero eps: the first segment starts at eps, not 0. 90_000 timescale;
        // eps 45000, segments 2s, 1s → presentation times 45000, 225000.
        let mut track = video_track(720, 128_000, &[180_000, 90_000]);
        track.header.earliest_presentation_time = 45_000;
        let repr = track.id();
        let m = build_media(&track).to_string();

        // First segment named by eps itself, not 0.
        assert!(m.contains(&format!("{repr}/45000.m4s")), "{m}");
        assert!(!m.contains(&format!("{repr}/0.m4s")), "{m}");
        // Second segment: eps + first duration = 45000 + 180000 = 225000.
        assert!(m.contains(&format!("{repr}/225000.m4s")), "{m}");
        // The EXT-X-MAP init still rides the first (offset) segment only.
        assert_eq!(m.matches("#EXT-X-MAP").count(), 1, "{m}");
    }

    #[test]
    fn target_duration_rounds_fractional_segment_up() {
        // 135_000 units @ 90_000 = 1.5s → ceil → 2 (proves .ceil() rounds up
        // rather than truncating to 1).
        let track = video_track(720, 128_000, &[135_000]);
        let m = build_media(&track).to_string();
        assert!(m.contains("#EXT-X-TARGETDURATION:2"), "{m}");
    }
}
