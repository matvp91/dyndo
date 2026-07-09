//! Maps a segment's `$Time$` (start presentation time, track timescale) to its
//! byte range in the source. Mirrors the SegmentTimeline semantics: the running
//! time starts at `earliest_presentation_time` and accumulates segment durations.

use dyndo_core::{ByteRange, CmafHeader};

/// The `[offset, offset+size)` byte range of the segment whose start presentation
/// time equals `time`, or `None` if no segment boundary falls on `time`.
pub fn segment_range(header: &CmafHeader, time: u64) -> Option<ByteRange> {
    let mut t = header.earliest_presentation_time;
    for seg in &header.segments {
        if t == time {
            return Some(ByteRange {
                start: seg.offset,
                end: seg.offset + seg.size,
            });
        }
        t += seg.duration;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyndo_core::{Segment, Stream, VideoCodec, VideoStream};

    fn header(ept: u64, segs: Vec<Segment>) -> CmafHeader {
        CmafHeader {
            timescale: 90000,
            duration: segs.iter().map(|s| s.duration).sum(),
            bandwidth: 1000,
            earliest_presentation_time: ept,
            init_range: ByteRange { start: 0, end: 100 },
            segments: segs,
            stream: Stream::Video(VideoStream {
                codec: VideoCodec::Avc {
                    profile: 0x64,
                    constraints: 0,
                    level: 0x28,
                },
                width: 1920,
                height: 1080,
                frame_rate: (25, 1),
            }),
        }
    }

    fn seg(offset: u64, size: u64, duration: u64) -> Segment {
        Segment {
            offset,
            size,
            duration,
        }
    }

    #[test]
    fn resolves_boundaries_and_rejects_misses() {
        let h = header(
            0,
            vec![seg(1000, 500, 90000), seg(1500, 700, 90000), seg(2200, 300, 45000)],
        );
        // First segment at t == ept.
        assert_eq!(segment_range(&h, 0), Some(ByteRange { start: 1000, end: 1500 }));
        // Second at ept + 90000.
        assert_eq!(segment_range(&h, 90000), Some(ByteRange { start: 1500, end: 2200 }));
        // Third at ept + 180000.
        assert_eq!(segment_range(&h, 180000), Some(ByteRange { start: 2200, end: 2500 }));
        // A time between boundaries matches nothing.
        assert_eq!(segment_range(&h, 45000), None);
        assert_eq!(segment_range(&h, 999999), None);
    }

    #[test]
    fn honours_nonzero_earliest_presentation_time() {
        let h = header(5000, vec![seg(10, 20, 90000)]);
        assert_eq!(segment_range(&h, 5000), Some(ByteRange { start: 10, end: 30 }));
        assert_eq!(segment_range(&h, 0), None);
    }
}
