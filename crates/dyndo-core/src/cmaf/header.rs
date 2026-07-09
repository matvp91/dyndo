//! Async, header-first CMAF parsing. Reads moov + sidx, then stops (never touches
//! a moof or mdat). Returns an internal `CmafHeader`; mapping to the serde model
//! lives in `asset.rs`.

use std::io::Cursor;

use mp4_atom::{
    Atom, Audio, Codec, FourCC, Header, Mdhd, Moov, ReadAtom, ReadFrom, Sidx, Trak, Visual,
};

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
pub struct VideoMeta {
    pub fourcc: &'static str,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioMeta {
    pub fourcc: &'static str,
    pub sample_rate: u32,
    pub channels: u16,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackMeta {
    Video(VideoMeta),
    Audio(AudioMeta),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CmafHeader {
    pub timescale: u32,
    pub duration: u64,
    /// Average bitrate in bits/s, derived from the segment sizes and duration.
    pub bandwidth: u32,
    pub init_range: ByteRange,
    pub segments: Vec<Segment>,
    pub track: TrackMeta,
}

fn malformed(path: &str, box_type: &str, reason: impl Into<String>) -> Error {
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
    moov_end: u64,
    sidx_end: u64,
}

/// Header-first scan: read moov and sidx; skip everything else (notably moof and
/// mdat, which we never fetch). Stops as soon as both are seen.
async fn scan_header_boxes<S: Source>(source: &S, path: &str) -> Result<ScannedBoxes> {
    let mut offset = 0u64;
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    while moov.is_none() || sidx.is_none() {
        let Some(frame) = next_box(source, offset, path).await? else {
            break; // reached end without the boxes we need
        };
        if frame.header.kind == Moov::KIND {
            moov = Some(read_atom_body(source, &frame, "moov", path).await?);
            moov_end = frame.box_end;
        } else if frame.header.kind == Sidx::KIND {
            sidx = Some(read_atom_body(source, &frame, "sidx", path).await?);
            sidx_end = frame.box_end;
        }
        offset = frame.box_end;
    }

    Ok(ScannedBoxes {
        moov: moov.ok_or_else(|| Error::MissingMoov(path.into()))?,
        sidx: sidx.ok_or_else(|| Error::MissingSidx(path.into()))?,
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

/// The sample-entry fourcc and shared visual box for the first supported video
/// codec in the `stsd`. The fourcc is the codec identity; assembling the RFC6381
/// codec string (profile/level/…) is a downstream manifest concern.
fn video_fourcc_and_visual<'a>(
    codecs: &'a [Codec],
    path: &str,
) -> Result<(&'static str, &'a Visual)> {
    codecs
        .iter()
        .find_map(|c| match c {
            Codec::Avc1(avc1) => Some(("avc1", &avc1.visual)),
            Codec::Av01(av01) => Some(("av01", &av01.visual)),
            _ => None,
        })
        .ok_or_else(|| malformed(path, "stsd", "no supported video sample entry"))
}

/// The sample-entry fourcc and shared audio box for the first supported audio
/// codec in the `stsd`.
fn audio_fourcc_and_audio<'a>(
    codecs: &'a [Codec],
    path: &str,
) -> Result<(&'static str, &'a Audio)> {
    codecs
        .iter()
        .find_map(|c| match c {
            Codec::Mp4a(mp4a) => Some(("mp4a", &mp4a.audio)),
            Codec::Ac3(ac3) => Some(("ac-3", &ac3.audio)),
            Codec::Eac3(eac3) => Some(("ec-3", &eac3.audio)),
            _ => None,
        })
        .ok_or_else(|| malformed(path, "stsd", "no supported audio sample entry"))
}

/// Project the single track's boxes into a codec-agnostic `TrackMeta`. The codec is
/// identified by the sample-entry variant, not the handler (which only says video/audio).
fn extract_track_meta(trak: &Trak, path: &str) -> Result<TrackMeta> {
    let mdia = &trak.mdia;
    let handler = mdia.hdlr.handler;
    let codecs = &mdia.minf.stbl.stsd.codecs;

    if handler == FourCC::new(b"vide") {
        let (fourcc, visual) = video_fourcc_and_visual(codecs, path)?;
        Ok(TrackMeta::Video(VideoMeta {
            fourcc,
            width: visual.width as u32,
            height: visual.height as u32,
        }))
    } else if handler == FourCC::new(b"soun") {
        let (fourcc, audio) = audio_fourcc_and_audio(codecs, path)?;
        Ok(TrackMeta::Audio(AudioMeta {
            fourcc,
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
    let track = extract_track_meta(trak, path)?;

    Ok(CmafHeader {
        timescale: scanned.sidx.timescale,
        duration,
        bandwidth,
        init_range: ByteRange {
            start: 0,
            end: scanned.moov_end,
        },
        segments,
        track,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalFile;
    use mp4_atom::{Ac3, Ac3SpecificBox, Av01, Eac3, Ec3IndependentSubstream, Ec3SpecificBox};

    fn fixture(name: &str) -> LocalFile {
        LocalFile::new(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
    }

    #[tokio::test]
    async fn reads_video_header() {
        let src = fixture("video_avc_1080.mp4");
        let h = read_header(&src, "video_avc_1080.mp4").await.unwrap();
        assert_eq!(h.timescale, 90000);
        assert_eq!(h.duration, 123328800);
        assert_eq!(h.bandwidth, 4807228);
        assert_eq!(h.segments.len(), 715);
        assert_eq!(h.init_range.start, 0);
        assert_eq!(h.init_range.end, 766);
        assert_eq!(h.segments[0].offset, 9386);
        assert_eq!(h.segments[0].size, 1495550);
        assert_eq!(h.segments[0].duration, 172800);
        match h.track {
            TrackMeta::Video(VideoMeta {
                fourcc,
                width,
                height,
            }) => {
                assert_eq!(fourcc, "avc1");
                assert_eq!((width, height), (1920, 1080));
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn reads_audio_header() {
        let src = fixture("audio_aac_nl_2.mp4");
        let h = read_header(&src, "audio_aac_nl_2.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.duration, 65775616);
        assert_eq!(h.bandwidth, 196918);
        assert_eq!(h.segments.len(), 715);
        assert_eq!(h.init_range.end, 662);
        assert_eq!(h.segments[0].offset, 9282);
        assert_eq!(h.segments[0].size, 48530);
        assert!(h.segments[0].duration > 0);
        match h.track {
            TrackMeta::Audio(AudioMeta {
                fourcc,
                sample_rate,
                channels,
                language,
            }) => {
                assert_eq!(fourcc, "mp4a");
                assert_eq!(sample_rate, 48000);
                assert_eq!(channels, 2);
                assert_eq!(language.as_deref(), Some("nld"));
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
        assert_eq!(h.timescale, 12800);
        assert_eq!(h.duration, 25600);
        assert_eq!(h.segments.len(), 2);
        assert_eq!(h.init_range.end, 783);
        assert_eq!(h.segments[0].offset, 847);
        assert_eq!(h.segments[0].size, 8342);
        assert_eq!(h.segments[0].duration, 12800);
        match h.track {
            TrackMeta::Video(VideoMeta {
                fourcc,
                width,
                height,
            }) => {
                assert_eq!(fourcc, "av01");
                assert_eq!((width, height), (320, 240));
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn reads_ac3_audio_header() {
        let src = fixture("audio_ac3_1.mp4");
        let h = read_header(&src, "audio_ac3_1.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.segments.len(), 3);
        assert_eq!(h.init_range.end, 725);
        assert_eq!(h.segments[0].offset, 801);
        assert_eq!(h.segments[0].size, 24684);
        match h.track {
            TrackMeta::Audio(AudioMeta {
                fourcc,
                sample_rate,
                channels,
                ..
            }) => {
                assert_eq!(fourcc, "ac-3");
                assert_eq!(sample_rate, 48000);
                assert_eq!(channels, 1);
            }
            _ => panic!("expected audio"),
        }
    }

    #[tokio::test]
    async fn reads_ec3_audio_header() {
        let src = fixture("audio_ec3_1.mp4");
        let h = read_header(&src, "audio_ec3_1.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.segments.len(), 3);
        assert_eq!(h.init_range.end, 727);
        assert_eq!(h.segments[0].offset, 803);
        assert_eq!(h.segments[0].size, 24684);
        match h.track {
            TrackMeta::Audio(AudioMeta {
                fourcc,
                sample_rate,
                channels,
                ..
            }) => {
                assert_eq!(fourcc, "ec-3");
                assert_eq!(sample_rate, 48000);
                assert_eq!(channels, 1);
            }
            _ => panic!("expected audio"),
        }
    }

    // Dispatch-level tests for the newer codecs. They exercise the real
    // sample-entry extraction without needing binary fixtures.

    #[test]
    fn video_dispatch_maps_av01_to_fourcc_and_visual() {
        let av01 = Av01 {
            visual: Visual {
                width: 1920,
                height: 800,
                ..Default::default()
            },
            ..Default::default()
        };
        let codecs = vec![Codec::Av01(av01)];
        let (fourcc, visual) = video_fourcc_and_visual(&codecs, "test.mp4").unwrap();
        assert_eq!(fourcc, "av01");
        assert_eq!((visual.width, visual.height), (1920, 800));
    }

    #[test]
    fn audio_dispatch_maps_ac3_to_fourcc_and_audio() {
        let ac3 = Ac3 {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 6,
                sample_size: 16,
                sample_rate: 48000.into(),
            },
            dac3: Ac3SpecificBox {
                fscod: 0,
                bsid: 8,
                bsmod: 0,
                acmod: 7,
                lfeon: true,
                bit_rate_code: 0,
            },
        };
        let codecs = vec![Codec::Ac3(ac3)];
        let (fourcc, audio) = audio_fourcc_and_audio(&codecs, "test.mp4").unwrap();
        assert_eq!(fourcc, "ac-3");
        assert_eq!(audio.channel_count, 6);
        assert_eq!(audio.sample_rate.integer() as u32, 48000);
    }

    #[test]
    fn audio_dispatch_maps_eac3_to_fourcc_and_audio() {
        let eac3 = Eac3 {
            audio: Audio {
                data_reference_index: 1,
                channel_count: 8,
                sample_size: 16,
                sample_rate: 48000.into(),
            },
            dec3: Ec3SpecificBox {
                data_rate: 768,
                substreams: vec![Ec3IndependentSubstream {
                    fscod: 0,
                    bsid: 16,
                    asvc: false,
                    bsmod: 0,
                    acmod: 7,
                    lfeon: true,
                    num_dep_sub: 0,
                    chan_loc: None,
                }],
            },
        };
        let codecs = vec![Codec::Eac3(eac3)];
        let (fourcc, audio) = audio_fourcc_and_audio(&codecs, "test.mp4").unwrap();
        assert_eq!(fourcc, "ec-3");
        assert_eq!(audio.channel_count, 8);
    }

    #[test]
    fn video_dispatch_errors_when_no_supported_entry() {
        assert!(video_fourcc_and_visual(&[], "test.mp4").is_err());
    }

    #[test]
    fn audio_dispatch_errors_when_no_supported_entry() {
        assert!(audio_fourcc_and_audio(&[], "test.mp4").is_err());
    }
}
