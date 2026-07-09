//! Async, header-first CMAF parsing. Reads moov + sidx + the first moof, then
//! stops (never touches mdat). The first moof supplies the video sample duration
//! used to derive frame rate. Returns an internal `CmafHeader`; mapping to the
//! serde model lives in `asset.rs`.

use std::io::Cursor;

use mp4_atom::{Atom, FourCC, Header, Mdhd, Moof, Moov, ReadAtom, ReadFrom, Sidx};

use crate::cmaf::codec::{self, AudioCodec, VideoCodec};
use crate::error::{Error, Result};
use crate::storage::Source;

#[derive(Debug, Clone, PartialEq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub offset: u64,
    pub size: u64,
    pub duration: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CmafHeader {
    Video(VideoCmafHeader),
    Audio(AudioCmafHeader),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoCmafHeader {
    pub timescale: u32,
    pub duration: u64,
    /// Average bitrate in bits/s, derived from the segment sizes and duration.
    pub bandwidth: u32,
    pub earliest_presentation_time: u64,
    pub init_range: ByteRange,
    pub segments: Vec<Segment>,
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub frame_rate: (u32, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioCmafHeader {
    pub timescale: u32,
    pub duration: u64,
    /// Average bitrate in bits/s, derived from the segment sizes and duration.
    pub bandwidth: u32,
    pub earliest_presentation_time: u64,
    pub init_range: ByteRange,
    pub segments: Vec<Segment>,
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: u16,
    pub language: Option<String>,
}

pub(crate) fn malformed(path: &str, box_type: &str, reason: impl Into<String>) -> Error {
    Error::MalformedBox {
        box_type: box_type.into(),
        path: path.into(),
        reason: reason.into(),
    }
}

/// ISO-639-2 language from `mdhd`; `"und"` (undetermined) and empty map to `None`.
fn language_string(mdhd: &Mdhd) -> Option<String> {
    match mdhd.language.as_str() {
        "" | "und" => None,
        s => Some(s.to_string()),
    }
}

/// Average bitrate in bits/s from the total segment bytes and the media duration.
fn average_bandwidth(total_bytes: u64, duration: u64, timescale: u32) -> u32 {
    if duration == 0 || timescale == 0 {
        return 0;
    }
    let seconds = duration as f64 / timescale as f64;
    (total_bytes as f64 * 8.0 / seconds).round() as u32
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// `(num, den)` frame rate = `timescale / sample_duration`, reduced.
fn frame_rate(sample_duration: u32, timescale: u32) -> (u32, u32) {
    if sample_duration == 0 || timescale == 0 {
        return (0, 1);
    }
    let g = gcd(timescale, sample_duration);
    (timescale / g, sample_duration / g)
}

/// First sample's duration from the first moof, falling back to tfhd then trex defaults.
fn first_sample_duration(moof: &Moof, moov: &Moov) -> u32 {
    let from_traf = moof.traf.first().and_then(|traf| {
        traf.trun
            .iter()
            .flat_map(|t| &t.entries)
            .find_map(|e| e.duration)
            .or(traf.tfhd.default_sample_duration)
    });
    from_traf
        .or_else(|| {
            moov.mvex
                .as_ref()
                .and_then(|m| m.trex.first())
                .map(|t| t.default_sample_duration)
        })
        .unwrap_or(0)
}

/// A single box's parsed header plus the absolute byte range of its body.
/// The body spans `[body_start, box_end)`; `body_len()` is that range's length
/// (equivalently `header.size`, which excludes the header per ISO-BMFF).
struct BoxFrame {
    header: Header,
    body_start: u64,
    box_end: u64,
}

impl BoxFrame {
    fn body_len(&self) -> usize {
        (self.box_end - self.body_start) as usize
    }
}

/// Read and parse the next box header at `offset`, computing its byte span.
/// Returns `None` at end-of-source (fewer than 8 bytes remain).
async fn next_box<S: Source>(source: &S, offset: u64, path: &str) -> Result<Option<BoxFrame>> {
    let head_bytes = source.read_at(offset, 16).await?;
    if head_bytes.len() < 8 {
        return Ok(None); // reached end without a full box header
    }
    let mut cursor = Cursor::new(&head_bytes[..]);
    let header =
        Header::read_from(&mut cursor).map_err(|e| malformed(path, "box header", e.to_string()))?;
    let header_len = cursor.position();
    let body_len = header
        .size
        .ok_or_else(|| malformed(path, "box", "unbounded box size"))?;
    let body_start = offset
        .checked_add(header_len)
        .ok_or_else(|| malformed(path, "box", "box size overflow"))?;
    let box_end = body_start
        .checked_add(body_len as u64)
        .ok_or_else(|| malformed(path, "box", "box size overflow"))?;
    Ok(Some(BoxFrame {
        header,
        body_start,
        box_end,
    }))
}

/// Fetch a box body and decode it into atom `A`.
async fn read_atom_body<A: ReadAtom, S: Source>(
    source: &S,
    frame: &BoxFrame,
    name: &str,
    path: &str,
) -> Result<A> {
    let body = source.read_at(frame.body_start, frame.body_len()).await?;
    A::read_atom(&frame.header, &mut Cursor::new(&body[..]))
        .map_err(|e| malformed(path, name, e.to_string()))
}

/// The header boxes we care about, with the byte offsets they end at.
struct ScannedBoxes {
    moov: Moov,
    sidx: Sidx,
    first_moof: Moof,
    moov_end: u64,
    sidx_end: u64,
}

/// Header-first scan: read moov, sidx and the first moof; skip everything else
/// (notably mdat, which we never fetch). Stops as soon as all three are seen.
/// The first moof supplies the video sample duration used to derive frame rate.
async fn scan_header_boxes<S: Source>(source: &S, path: &str) -> Result<ScannedBoxes> {
    let mut offset = 0u64;
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut first_moof: Option<Moof> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    while moov.is_none() || sidx.is_none() || first_moof.is_none() {
        let Some(frame) = next_box(source, offset, path).await? else {
            break; // reached end without the boxes we need
        };
        if frame.header.kind == Moov::KIND {
            moov = Some(read_atom_body(source, &frame, "moov", path).await?);
            moov_end = frame.box_end;
        } else if frame.header.kind == Sidx::KIND {
            sidx = Some(read_atom_body(source, &frame, "sidx", path).await?);
            sidx_end = frame.box_end;
        } else if frame.header.kind == Moof::KIND && first_moof.is_none() {
            first_moof = Some(read_atom_body(source, &frame, "moof", path).await?);
        }
        offset = frame.box_end;
    }

    Ok(ScannedBoxes {
        moov: moov.ok_or_else(|| Error::MissingMoov(path.into()))?,
        sidx: sidx.ok_or_else(|| Error::MissingSidx(path.into()))?,
        first_moof: first_moof.ok_or_else(|| malformed(path, "moof", "missing first moof"))?,
        moov_end,
        sidx_end,
    })
}

/// Absolute segment map from `sidx`: each reference's cumulative offset, size and duration.
fn build_segments(sidx: &Sidx, sidx_end: u64) -> Vec<Segment> {
    let mut seg_offset = sidx_end + sidx.first_offset;
    let mut segments = Vec::with_capacity(sidx.references.len());
    for r in &sidx.references {
        segments.push(Segment {
            offset: seg_offset,
            size: r.reference_size as u64,
            duration: r.subsegment_duration as u64,
        });
        seg_offset += r.reference_size as u64;
    }
    segments
}

pub async fn read_header<S: Source>(source: &S, path: &str) -> Result<CmafHeader> {
    let scanned = scan_header_boxes(source, path).await?;

    // Exactly one track.
    if scanned.moov.trak.len() != 1 {
        return Err(Error::NotSingleTrack {
            path: path.into(),
            count: scanned.moov.trak.len(),
        });
    }
    let trak = &scanned.moov.trak[0];

    let segments = build_segments(&scanned.sidx, scanned.sidx_end);
    let duration: u64 = segments.iter().map(|s| s.duration).sum();
    let total_bytes: u64 = segments.iter().map(|s| s.size).sum();
    let bandwidth = average_bandwidth(total_bytes, duration, scanned.sidx.timescale);

    let mdia = &trak.mdia;
    let handler = mdia.hdlr.handler;
    let codecs = &mdia.minf.stbl.stsd.codecs;
    let init_range = ByteRange {
        start: 0,
        end: scanned.moov_end,
    };

    if handler == FourCC::new(b"vide") {
        let (codec, visual) = codec::video_codec(codecs, path)?;
        let sample_duration = first_sample_duration(&scanned.first_moof, &scanned.moov);
        // frame_rate divides the moof sample duration by the sidx timescale; conformant
        // CMAF has sidx.timescale == mdhd (media) timescale, so this is exact.
        let frame_rate = frame_rate(sample_duration, scanned.sidx.timescale);
        Ok(CmafHeader::Video(VideoCmafHeader {
            timescale: scanned.sidx.timescale,
            duration,
            bandwidth,
            earliest_presentation_time: scanned.sidx.earliest_presentation_time,
            init_range,
            segments,
            codec,
            width: visual.width as u32,
            height: visual.height as u32,
            frame_rate,
        }))
    } else if handler == FourCC::new(b"soun") {
        let (codec, audio) = codec::audio_codec(codecs, path)?;
        Ok(CmafHeader::Audio(AudioCmafHeader {
            timescale: scanned.sidx.timescale,
            duration,
            bandwidth,
            earliest_presentation_time: scanned.sidx.earliest_presentation_time,
            init_range,
            segments,
            codec,
            sample_rate: audio.sample_rate.integer() as u32,
            channels: audio.channel_count,
            language: language_string(&mdia.mdhd),
        }))
    } else {
        Err(Error::UnsupportedCodec {
            path: path.into(),
            codec: format!("{:?}", handler),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalFile;

    fn fixture(name: &str) -> LocalFile {
        LocalFile::new(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
    }

    #[test]
    fn frame_rate_reduces_to_lowest_terms() {
        assert_eq!(frame_rate(3600, 90000), (25, 1));
        assert_eq!(frame_rate(1001, 30000), (30000, 1001));
        assert_eq!(frame_rate(0, 90000), (0, 1));
    }

    #[tokio::test]
    async fn reads_video_header() {
        let src = fixture("video_avc_1080.mp4");
        let h = read_header(&src, "video_avc_1080.mp4").await.unwrap();
        match h {
            CmafHeader::Video(v) => {
                assert_eq!(v.timescale, 90000);
                assert_eq!(v.duration, 123328800);
                assert_eq!(v.bandwidth, 4807228);
                assert_eq!(v.earliest_presentation_time, 0);
                assert_eq!(v.segments.len(), 715);
                assert_eq!(v.init_range.start, 0);
                assert_eq!(v.init_range.end, 766);
                assert_eq!(v.segments[0].offset, 9386);
                assert_eq!(v.segments[0].size, 1495550);
                assert_eq!(v.segments[0].duration, 172800);
                assert_eq!(v.codec.fourcc(), "avc1");
                assert_eq!(v.codec.rfc6381(), "avc1.640028");
                assert_eq!((v.width, v.height), (1920, 1080));
                assert_eq!(v.frame_rate, (25, 1));
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn reads_audio_header() {
        let src = fixture("audio_aac_nl_2.mp4");
        let h = read_header(&src, "audio_aac_nl_2.mp4").await.unwrap();
        match h {
            CmafHeader::Audio(a) => {
                assert_eq!(a.timescale, 48000);
                assert_eq!(a.duration, 65775616);
                assert_eq!(a.bandwidth, 196918);
                assert_eq!(a.earliest_presentation_time, 0);
                assert_eq!(a.segments.len(), 715);
                assert_eq!(a.init_range.end, 662);
                assert_eq!(a.segments[0].offset, 9282);
                assert_eq!(a.segments[0].size, 48530);
                assert!(a.segments[0].duration > 0);
                assert_eq!(a.codec.fourcc(), "mp4a");
                assert_eq!(a.codec.rfc6381(), "mp4a.40.2");
                assert_eq!(a.sample_rate, 48000);
                assert_eq!(a.channels, 2);
                assert_eq!(a.language.as_deref(), Some("nld"));
            }
            _ => panic!("expected audio"),
        }
    }

    // End-to-end fixtures for the newer codecs. Each is a real ffmpeg-muxed CMAF
    // file truncated at the end of the first moof (ftyp+moov+sidx+moof, no mdat).
    // Expected values were computed independently (sidx/av1C parse + ffprobe).

    #[tokio::test]
    async fn reads_av1_video_header() {
        let src = fixture("video_av1_240.mp4");
        let h = read_header(&src, "video_av1_240.mp4").await.unwrap();
        match h {
            CmafHeader::Video(v) => {
                assert_eq!(v.timescale, 12800);
                assert_eq!(v.duration, 25600);
                assert_eq!(v.segments.len(), 2);
                assert_eq!(v.init_range.end, 783);
                assert_eq!(v.segments[0].offset, 847);
                assert_eq!(v.segments[0].size, 8342);
                assert_eq!(v.segments[0].duration, 12800);
                assert_eq!(v.codec.fourcc(), "av01");
                assert_eq!((v.width, v.height), (320, 240));
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn reads_ac3_audio_header() {
        let src = fixture("audio_ac3_1.mp4");
        let h = read_header(&src, "audio_ac3_1.mp4").await.unwrap();
        match h {
            CmafHeader::Audio(a) => {
                assert_eq!(a.timescale, 48000);
                assert_eq!(a.segments.len(), 3);
                assert_eq!(a.init_range.end, 725);
                assert_eq!(a.segments[0].offset, 801);
                assert_eq!(a.segments[0].size, 24684);
                assert_eq!(a.codec.fourcc(), "ac-3");
                assert_eq!(a.sample_rate, 48000);
                assert_eq!(a.channels, 1);
            }
            _ => panic!("expected audio"),
        }
    }

    #[tokio::test]
    async fn reads_ec3_audio_header() {
        let src = fixture("audio_ec3_1.mp4");
        let h = read_header(&src, "audio_ec3_1.mp4").await.unwrap();
        match h {
            CmafHeader::Audio(a) => {
                assert_eq!(a.timescale, 48000);
                assert_eq!(a.segments.len(), 3);
                assert_eq!(a.init_range.end, 727);
                assert_eq!(a.segments[0].offset, 803);
                assert_eq!(a.segments[0].size, 24684);
                assert_eq!(a.codec.fourcc(), "ec-3");
                assert_eq!(a.sample_rate, 48000);
                assert_eq!(a.channels, 1);
            }
            _ => panic!("expected audio"),
        }
    }
}
