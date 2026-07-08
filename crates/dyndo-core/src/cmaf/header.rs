//! Async, header-first CMAF parsing. Reads moov + sidx + first moof, then stops
//! (never touches an mdat). Returns an internal `CmafHeader`; mapping to the
//! serde model lives in `asset.rs`.

use std::io::Cursor;

use mp4_atom::{
    Atom, Avc1, Codec, FourCC, Header, Mdhd, Moof, Moov, Mp4a, ReadAtom, ReadFrom, Sidx, Stsd,
};

use crate::cmaf::codec::{aac_codec_string, avc_codec_string};
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
pub enum TrackMeta {
    Video {
        codec: String,
        width: u32,
        height: u32,
        frame_rate: String,
    },
    Audio {
        codec: String,
        sample_rate: u32,
        channels: u16,
        language: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CmafHeader {
    pub timescale: u32,
    pub duration: u64,
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

/// Reduce a `num/den` fraction to lowest terms as a `"n/d"` string.
fn frame_rate_string(timescale: u32, sample_duration: u32) -> String {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
    if sample_duration == 0 {
        return format!("{}/1", timescale);
    }
    let g = gcd(timescale, sample_duration).max(1);
    format!("{}/{}", timescale / g, sample_duration / g)
}

fn is_video(handler: &FourCC) -> bool {
    *handler == FourCC::new(b"vide")
}

fn is_audio(handler: &FourCC) -> bool {
    *handler == FourCC::new(b"soun")
}

/// The single AVC sample entry in an `stsd`, if present.
fn video_sample_entry(stsd: &Stsd) -> Option<&Avc1> {
    stsd.codecs.iter().find_map(|c| match c {
        Codec::Avc1(a) => Some(a),
        _ => None,
    })
}

/// The single AAC sample entry in an `stsd`, if present.
fn audio_sample_entry(stsd: &Stsd) -> Option<&Mp4a> {
    stsd.codecs.iter().find_map(|c| match c {
        Codec::Mp4a(a) => Some(a),
        _ => None,
    })
}

/// AAC audio object type from the decoder-specific info (AAC-LC == 2).
fn audio_object_type(mp4a: &Mp4a) -> u8 {
    mp4a.esds.es_desc.dec_config.dec_specific.profile
}

/// ISO-639-2 language from `mdhd`; `"und"` (undetermined) and empty map to `None`.
fn language_string(mdhd: &Mdhd) -> Option<String> {
    match mdhd.language.as_str() {
        "" | "und" => None,
        s => Some(s.to_string()),
    }
}

/// Per-sample duration of the first fragment: `tfhd.default_sample_duration`
/// if set, else the first `trun` entry's duration; `0` when there is no moof.
fn first_sample_duration(moof: Option<&Moof>) -> u32 {
    let Some(moof) = moof else { return 0 };
    let Some(traf) = moof.traf.first() else {
        return 0;
    };
    if let Some(d) = traf.tfhd.default_sample_duration {
        return d;
    }
    traf.trun
        .first()
        .and_then(|t| t.entries.first())
        .and_then(|e| e.duration)
        .unwrap_or(0)
}

pub async fn read_header<S: Source>(source: &S, path: &str) -> Result<CmafHeader> {
    let mut offset = 0u64;
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut first_moof: Option<Moof> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    // Header-first scan: read moov, sidx, first moof; skip everything else
    // (notably mdat bodies, which we never fetch).
    while moov.is_none() || sidx.is_none() || first_moof.is_none() {
        let head_bytes = source.read_at(offset, 16).await?;
        if head_bytes.len() < 8 {
            break; // reached end without the boxes we need
        }
        let mut cursor = Cursor::new(&head_bytes[..]);
        let header = Header::read_from(&mut cursor)
            .map_err(|e| malformed(path, "box header", e.to_string()))?;
        let header_len = cursor.position();
        let body_len = header
            .size
            .ok_or_else(|| malformed(path, "box", "unbounded box size"))?;
        let body_start = offset + header_len;
        let box_end = body_start + body_len as u64;

        if header.kind == Moov::KIND {
            let body = source.read_at(body_start, body_len).await?;
            moov = Some(
                Moov::read_atom(&header, &mut Cursor::new(&body[..]))
                    .map_err(|e| malformed(path, "moov", e.to_string()))?,
            );
            moov_end = box_end;
        } else if header.kind == Sidx::KIND {
            let body = source.read_at(body_start, body_len).await?;
            sidx = Some(
                Sidx::read_atom(&header, &mut Cursor::new(&body[..]))
                    .map_err(|e| malformed(path, "sidx", e.to_string()))?,
            );
            sidx_end = box_end;
        } else if header.kind == Moof::KIND {
            let body = source.read_at(body_start, body_len).await?;
            first_moof = Some(
                Moof::read_atom(&header, &mut Cursor::new(&body[..]))
                    .map_err(|e| malformed(path, "moof", e.to_string()))?,
            );
        }
        offset = box_end;
    }

    let moov = moov.ok_or_else(|| Error::MissingMoov(path.into()))?;
    let sidx = sidx.ok_or_else(|| Error::MissingSidx(path.into()))?;

    // Exactly one track.
    if moov.trak.len() != 1 {
        return Err(Error::NotSingleTrack {
            path: path.into(),
            count: moov.trak.len(),
        });
    }
    let trak = &moov.trak[0];

    // Segment map from sidx.
    let timescale = sidx.timescale;
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
    let duration: u64 = segments.iter().map(|s| s.duration).sum();

    // Track metadata.
    let handler = &trak.mdia.hdlr.handler;
    let media_timescale = trak.mdia.mdhd.timescale;
    let stsd = &trak.mdia.minf.stbl.stsd;

    let track = if is_video(handler) {
        let avc1 = video_sample_entry(stsd)
            .ok_or_else(|| malformed(path, "stsd", "no avc1 sample entry"))?;
        let avcc = &avc1.avcc;
        let codec = avc_codec_string(
            avcc.avc_profile_indication,
            avcc.profile_compatibility,
            avcc.avc_level_indication,
        );
        let sample_duration = first_sample_duration(first_moof.as_ref());
        TrackMeta::Video {
            codec,
            width: avc1.visual.width as u32,
            height: avc1.visual.height as u32,
            frame_rate: frame_rate_string(media_timescale, sample_duration),
        }
    } else if is_audio(handler) {
        let mp4a = audio_sample_entry(stsd)
            .ok_or_else(|| malformed(path, "stsd", "no mp4a sample entry"))?;
        let aot = audio_object_type(mp4a);
        TrackMeta::Audio {
            codec: aac_codec_string(aot),
            sample_rate: mp4a.audio.sample_rate.integer() as u32,
            channels: mp4a.audio.channel_count,
            language: language_string(&trak.mdia.mdhd),
        }
    } else {
        return Err(Error::UnsupportedCodec {
            path: path.into(),
            codec: format!("{:?}", handler),
        });
    };

    Ok(CmafHeader {
        timescale,
        duration,
        init_range: ByteRange {
            start: 0,
            end: moov_end,
        },
        segments,
        track,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::BytesSource;

    fn fixture(name: &str) -> BytesSource {
        let bytes = std::fs::read(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
        .unwrap();
        BytesSource::new(bytes)
    }

    #[tokio::test]
    async fn reads_video_header() {
        let src = fixture("index_video_avc_1080.mp4");
        let h = read_header(&src, "index_video_avc_1080.mp4").await.unwrap();
        assert_eq!(h.timescale, 90000);
        assert_eq!(h.duration, 123328800);
        assert_eq!(h.segments.len(), 715);
        assert_eq!(h.init_range.start, 0);
        match h.track {
            TrackMeta::Video {
                codec,
                width,
                height,
                frame_rate,
            } => {
                assert_eq!(codec, "avc1.640028");
                assert_eq!((width, height), (1920, 1080));
                assert_eq!(frame_rate, "25/1");
            }
            _ => panic!("expected video"),
        }
    }

    #[tokio::test]
    async fn reads_audio_header() {
        let src = fixture("index_audio_aac_nl_2.mp4");
        let h = read_header(&src, "index_audio_aac_nl_2.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.duration, 65775616);
        assert_eq!(h.segments.len(), 715);
        match h.track {
            TrackMeta::Audio {
                codec,
                sample_rate,
                channels,
                language,
            } => {
                assert_eq!(codec, "mp4a.40.2");
                assert_eq!(sample_rate, 48000);
                assert_eq!(channels, 2);
                assert_eq!(language.as_deref(), Some("nld"));
            }
            _ => panic!("expected audio"),
        }
    }
}
