use super::{CmafHeader, Segment};

pub fn find_segment_by_time(header: &CmafHeader, time: u64) -> Option<&Segment> {
    let mut t = header.earliest_presentation_time;
    for seg in &header.segments {
        if t == time {
            return Some(seg);
        }
        t += seg.duration;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{Segment, Stream, VideoCodec, VideoStream};

    fn header(ept: u64, segs: Vec<Segment>) -> CmafHeader {
        CmafHeader {
            timescale: 90000,
            duration: segs.iter().map(|s| s.duration).sum(),
            bandwidth: 1000,
            earliest_presentation_time: ept,
            init_segment: Segment { offset: 0, size: 100, duration: 0 },
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
        assert_eq!(find_segment_by_time(&h, 0), Some(&h.segments[0]));
        // Second at ept + 90000.
        assert_eq!(find_segment_by_time(&h, 90000), Some(&h.segments[1]));
        // Third at ept + 180000.
        assert_eq!(find_segment_by_time(&h, 180000), Some(&h.segments[2]));
        // A time between boundaries matches nothing.
        assert_eq!(find_segment_by_time(&h, 45000), None);
        assert_eq!(find_segment_by_time(&h, 999999), None);
    }

    #[test]
    fn honours_nonzero_earliest_presentation_time() {
        let h = header(5000, vec![seg(10, 20, 90000)]);
        assert_eq!(find_segment_by_time(&h, 5000), Some(&h.segments[0]));
        assert_eq!(find_segment_by_time(&h, 0), None);
    }
}
