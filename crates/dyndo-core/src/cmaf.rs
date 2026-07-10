//! A lightweight parse of a CMAF track's header region into a [`Header`] and
//! [`Metadata`], reading byte ranges through an operator.

use std::io::Cursor;

use mp4_atom::{Atom, FourCC, Header as BoxHeader, Mdhd, Moof, Moov, ReadAtom, ReadFrom, Sidx};
use opendal::Operator;

use crate::asset::Segment;
use crate::codec::{AudioCodec, VideoCodec};
use crate::CoreError;

/// Read `len` bytes of `path` starting at `offset`, through `op`.
pub(crate) async fn read(
    op: &Operator,
    path: &str,
    offset: u64,
    len: u64,
) -> Result<Vec<u8>, CoreError> {
    let buf = op.read_with(path).range(offset..offset + len).await?;
    Ok(buf.to_vec())
}

/// Fetch a box body and decode it into atom `A`.
async fn atom<A: ReadAtom>(
    op: &Operator,
    path: &str,
    bh: &BoxHeader,
    body_start: u64,
    body_len: u64,
) -> Result<A, CoreError> {
    let body = read(op, path, body_start, body_len).await?;
    A::read_atom(bh, &mut Cursor::new(&body[..])).map_err(|e| CoreError::Container(e.to_string()))
}

/// Scan the `moov`/`sidx`/first-`moof` boxes of the CMAF track at `path` and
/// project them into the common [`Header`] and the track's [`Metadata`]. `mdat`
/// is never fetched.
pub async fn header(
    op: &Operator,
    path: &str,
) -> Result<(Header, Vec<Segment>, Metadata), CoreError> {
    let mut offset = 0u64;
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut first_moof: Option<Moof> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    while moov.is_none() || sidx.is_none() || first_moof.is_none() {
        let head = read(op, path, offset, 16).await?;
        let mut cursor = Cursor::new(&head[..]);
        let bh =
            BoxHeader::read_from(&mut cursor).map_err(|e| CoreError::Container(e.to_string()))?;
        let body_start = offset + cursor.position();
        let body_len =
            bh.size
                .ok_or_else(|| CoreError::Container("box has no size".into()))? as u64;
        let box_end = body_start + body_len;

        if bh.kind == Moov::KIND {
            moov = Some(atom(op, path, &bh, body_start, body_len).await?);
            moov_end = box_end;
        } else if bh.kind == Sidx::KIND {
            sidx = Some(atom(op, path, &bh, body_start, body_len).await?);
            sidx_end = box_end;
        } else if bh.kind == Moof::KIND && first_moof.is_none() {
            first_moof = Some(atom(op, path, &bh, body_start, body_len).await?);
        }
        offset = box_end;
    }

    let moov = moov.expect("present: the loop exits only once all three boxes are set");
    let sidx = sidx.expect("present: the loop exits only once all three boxes are set");
    let first_moof = first_moof.expect("present: the loop exits only once all three boxes are set");

    let segments = segments(&sidx, sidx_end);
    let duration = segments.iter().map(|s| s.duration).sum();
    let total_bytes = segments.iter().map(|s| s.size).sum();
    let bandwidth = average_bandwidth(total_bytes, duration, sidx.timescale);

    let mdia = &moov.trak[0].mdia;
    let codecs = &mdia.minf.stbl.stsd.codecs;
    let metadata = if mdia.hdlr.handler == FourCC::new(b"vide") {
        let (codec, visual) = VideoCodec::from_codecs(codecs)?;
        let sample_duration = first_sample_duration(&first_moof, &moov);
        Metadata::Video(VideoMetadata {
            codec,
            width: visual.width as u32,
            height: visual.height as u32,
            frame_rate: frame_rate(sample_duration, sidx.timescale),
        })
    } else {
        let (codec, audio) = AudioCodec::from_codecs(codecs)?;
        Metadata::Audio(AudioMetadata {
            codec,
            sample_rate: audio.sample_rate.integer() as u32,
            channels: audio.channel_count,
            language: language_string(&mdia.mdhd),
        })
    };

    let header = Header {
        timescale: sidx.timescale,
        duration,
        bandwidth,
        earliest_presentation_time: sidx.earliest_presentation_time,
        init_segment: Segment {
            offset: 0,
            size: moov_end,
            duration: 0,
        },
    };
    Ok((header, segments, metadata))
}

fn segments(sidx: &Sidx, sidx_end: u64) -> Vec<Segment> {
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

fn frame_rate(sample_duration: u32, timescale: u32) -> (u32, u32) {
    if sample_duration == 0 || timescale == 0 {
        return (0, 1);
    }
    let g = gcd(timescale, sample_duration);
    (timescale / g, sample_duration / g)
}

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

/// Map an empty ISO-639-2 language code to the "undetermined" placeholder.
fn normalize_language(lang: &str) -> &str {
    if lang.is_empty() {
        "und"
    } else {
        lang
    }
}

fn language_string(mdhd: &Mdhd) -> String {
    normalize_language(mdhd.language.as_str()).to_string()
}

/// The fields common to every CMAF track's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub timescale: u32,
    pub duration: u64,
    /// Average bitrate in bits/s, derived from the segment sizes and duration.
    pub bandwidth: u32,
    pub earliest_presentation_time: u64,
    pub init_segment: Segment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Metadata {
    Video(VideoMetadata),
    Audio(AudioMetadata),
}

impl Metadata {
    /// Sample-entry fourcc (e.g. `"avc1"`, `"mp4a"`), regardless of media type.
    pub fn fourcc(&self) -> &'static str {
        match self {
            Metadata::Video(v) => v.codec.fourcc(),
            Metadata::Audio(a) => a.codec.fourcc(),
        }
    }

    /// RFC 6381 `codecs` string, regardless of media type.
    pub fn rfc6381(&self) -> String {
        match self {
            Metadata::Video(v) => v.codec.rfc6381(),
            Metadata::Audio(a) => a.codec.rfc6381(),
        }
    }

    /// The `video/mp4` / `audio/mp4` MIME type of this track's CMAF segments.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Metadata::Video(_) => "video/mp4",
            Metadata::Audio(_) => "audio/mp4",
        }
    }

    /// ISO-639-2 language (audio only; video is `None`).
    pub fn language(&self) -> Option<&str> {
        match self {
            Metadata::Video(_) => None,
            Metadata::Audio(a) => Some(&a.language),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoMetadata {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub frame_rate: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioMetadata {
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: u16,
    pub language: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcd_of_coprime_numbers_is_one() {
        assert_eq!(gcd(9, 4), 1);
    }

    #[test]
    fn gcd_extracts_the_common_factor() {
        assert_eq!(gcd(48_000, 1_600), 1_600);
    }

    #[test]
    fn frame_rate_reduces_to_lowest_terms() {
        // 48000 timescale / 1600 sample duration = 30 fps
        assert_eq!(frame_rate(1_600, 48_000), (30, 1));
    }

    #[test]
    fn frame_rate_is_zero_when_sample_duration_is_zero() {
        assert_eq!(frame_rate(0, 48_000), (0, 1));
    }

    #[test]
    fn average_bandwidth_is_zero_when_duration_is_zero() {
        assert_eq!(average_bandwidth(1_000, 0, 48_000), 0);
    }

    #[test]
    fn average_bandwidth_is_bits_per_second() {
        // 1000 bytes over exactly 1 second = 8000 bits/s
        assert_eq!(average_bandwidth(1_000, 48_000, 48_000), 8_000);
    }

    #[test]
    fn normalize_language_maps_empty_to_und() {
        assert_eq!(normalize_language(""), "und");
    }

    #[test]
    fn normalize_language_passes_through_a_known_code() {
        assert_eq!(normalize_language("nld"), "nld");
    }

    #[tokio::test]
    async fn header_returns_error_on_garbage_input_instead_of_panicking() {
        use opendal::services::Fs;
        use opendal::Operator;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.mp4"), [0xAA_u8; 64]).unwrap();
        let op = Operator::new(Fs::default().root(dir.path().to_str().unwrap())).unwrap();

        let result = header(&op, "bad.mp4").await;
        assert!(
            result.is_err(),
            "expected an error on garbage input, got Ok"
        );
    }
}
