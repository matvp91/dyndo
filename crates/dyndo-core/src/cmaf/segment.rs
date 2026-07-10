use mp4_atom::Sidx;

use super::CmafHeader;
use crate::error::Result;
use crate::storage::Source;

/// A (sub)segment's location in the file: its byte `offset` and `size`, plus
/// its `duration` in the track timescale (`0` for the init segment).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub offset: u64,
    pub size: u64,
    pub duration: u64,
}

pub(super) fn segments(sidx: &Sidx, sidx_end: u64) -> Vec<Segment> {
    let mut seg_offset = sidx_end + sidx.first_offset;
    let mut out = Vec::with_capacity(sidx.references.len());
    for r in &sidx.references {
        out.push(Segment {
            offset: seg_offset,
            size: r.reference_size as u64,
            duration: r.subsegment_duration as u64,
        });
        seg_offset += r.reference_size as u64;
    }
    out
}

/// The segment whose presentation time equals `time` (in the track timescale),
/// or `None` if no segment starts exactly at `time`.
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

/// Read `header`'s init segment (`ftyp`+`moov`) bytes from `source`; the `mdat`
/// boxes are never fetched.
pub async fn read_init_segment<S: Source>(source: &S, header: &CmafHeader) -> Result<Vec<u8>> {
    let r = &header.init_segment;
    source.read_at(r.offset, r.size as usize).await
}

/// Read the media (sub)segment of `header` that starts at presentation `time`
/// (in the track timescale) from `source`, or `None` if no segment starts
/// exactly there.
pub async fn read_segment<S: Source>(
    source: &S,
    header: &CmafHeader,
    time: u64,
) -> Result<Option<Vec<u8>>> {
    let Some(seg) = find_segment_by_time(header, time) else {
        return Ok(None);
    };
    let bytes = source.read_at(seg.offset, seg.size as usize).await?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmaf::{read_header, Segment, Stream, VideoCodec, VideoStream};

    fn header(ept: u64, segs: Vec<Segment>) -> CmafHeader {
        CmafHeader {
            timescale: 90000,
            duration: segs.iter().map(|s| s.duration).sum(),
            bandwidth: 1000,
            earliest_presentation_time: ept,
            init_segment: Segment {
                offset: 0,
                size: 100,
                duration: 0,
            },
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
            vec![
                seg(1000, 500, 90000),
                seg(1500, 700, 90000),
                seg(2200, 300, 45000),
            ],
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

    fn fixture(name: &str) -> crate::storage::LocalFile {
        crate::storage::LocalFile::new(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
    }

    #[tokio::test]
    async fn read_init_segment_returns_the_init_box_bytes() {
        let src = fixture("video_avc_1080.mp4");
        let header = read_header(&src, "video_avc_1080.mp4").await.unwrap();
        let bytes = read_init_segment(&src, &header).await.unwrap();
        // The init segment (ftyp+moov) is 766 bytes and fully present in the fixture.
        assert_eq!(bytes.len(), 766);
        // A CMAF init segment begins with the `ftyp` box: [size:4][type:4].
        assert_eq!(&bytes[4..8], b"ftyp");
    }

    #[tokio::test]
    async fn read_segment_returns_the_subsegment_at_that_time() {
        let src = fixture("video_avc_1080.mp4");
        let header = read_header(&src, "video_avc_1080.mp4").await.unwrap();
        let bytes = read_segment(&src, &header, 0)
            .await
            .unwrap()
            .expect("a segment starts at presentation time 0");
        // The subsegment begins with its `moof` box (the `mdat` that follows is
        // truncated in the fixture, but the header proves the offset is right).
        assert_eq!(&bytes[4..8], b"moof");
    }

    #[tokio::test]
    async fn read_segment_is_none_when_no_segment_starts_at_time() {
        let src = fixture("video_avc_1080.mp4");
        let header = read_header(&src, "video_avc_1080.mp4").await.unwrap();
        // 1 falls between the first two segment boundaries — no exact match.
        assert!(read_segment(&src, &header, 1).await.unwrap().is_none());
    }
}
