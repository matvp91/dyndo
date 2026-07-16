//! A lightweight parse of a CMAF track's header region into a [`CmafHeader`] and
//! [`CmafMetadata`], reading byte ranges through an operator.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use mp4_atom::{
    AsyncReadAtom, AsyncReadFrom, Atom, FourCC, Header as BoxHeader, Mdhd, Moof, Moov, Sidx,
};
use opendal::Operator;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::CoreError;
use crate::asset::Segment;
use crate::codec::{AudioCodec, TextCodec, VideoCodec};

/// Read `len` bytes of `path` starting at `offset`, through `op`. Returns
/// [`Bytes`] so a contiguous read is handed to the caller without copying.
pub(crate) async fn read_range(
    op: &Operator,
    path: &str,
    offset: u64,
    len: u64,
) -> Result<Bytes, CoreError> {
    let buf = op.read_with(path).range(offset..offset + len).await?;
    Ok(buf.to_bytes())
}

/// An [`AsyncRead`] that tallies every byte read through it, so the streamed
/// parse can record absolute box offsets (`moov`/`sidx` end) without seeking.
struct CountingReader<R> {
    inner: R,
    count: u64,
}

impl<R> CountingReader<R> {
    fn new(inner: R) -> Self {
        Self { inner, count: 0 }
    }

    /// Total bytes read through this reader so far.
    fn count(&self) -> u64 {
        self.count
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for CountingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            self.count += (buf.filled().len() - before) as u64;
        }
        poll
    }
}

/// Read and discard `len` bytes from `r`, erroring if the stream ends early.
async fn skip<R: AsyncRead + Unpin>(r: &mut R, len: u64) -> Result<(), CoreError> {
    let copied = tokio::io::copy(&mut r.take(len), &mut tokio::io::sink())
        .await
        .map_err(|e| CoreError::Container(e.to_string()))?;
    if copied != len {
        return Err(CoreError::Container("truncated box body".into()));
    }
    Ok(())
}

/// Scan the `moov`/`sidx`/first-`moof` boxes of the CMAF track at `path` and
/// project them into the common [`CmafHeader`] and the track's [`CmafMetadata`].
/// `mdat` is never fetched.
///
/// # Errors
/// Propagates any [`CoreError`] if a required box is missing, cannot be read
/// or parsed, if the track's media handler is neither video (`vide`) nor audio
/// (`soun`), or if the track's codec is unsupported.
pub async fn probe(op: &Operator, path: &str) -> Result<(CmafHeader, CmafMetadata), CoreError> {
    let reader = op
        .reader(path)
        .await?
        .into_futures_async_read(..)
        .await?
        .compat();
    let mut r = CountingReader::new(reader);

    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut moof: Option<Moof> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    while moov.is_none() || sidx.is_none() || moof.is_none() {
        let bh = BoxHeader::read_from(&mut r)
            .await
            .map_err(|e| CoreError::Container(e.to_string()))?;
        let body_len =
            bh.size
                .ok_or_else(|| CoreError::Container("box has no size".into()))? as u64;

        if bh.kind == Moov::KIND {
            moov = Some(
                Moov::read_atom(&bh, &mut r)
                    .await
                    .map_err(|e| CoreError::Container(e.to_string()))?,
            );
            moov_end = r.count();
        } else if bh.kind == Sidx::KIND {
            sidx = Some(
                Sidx::read_atom(&bh, &mut r)
                    .await
                    .map_err(|e| CoreError::Container(e.to_string()))?,
            );
            sidx_end = r.count();
        } else if bh.kind == Moof::KIND {
            // The first `moof` ends the header region; `mdat` follows it.
            moof = Some(
                Moof::read_atom(&bh, &mut r)
                    .await
                    .map_err(|e| CoreError::Container(e.to_string()))?,
            );
            break;
        } else {
            skip(&mut r, body_len).await?;
        }
    }

    let moov = moov.ok_or_else(|| CoreError::Container("missing moov before first moof".into()))?;
    let sidx = sidx.ok_or_else(|| CoreError::Container("missing sidx before first moof".into()))?;
    let moof = moof.ok_or_else(|| CoreError::Container("missing moof".into()))?;

    let segments = build_segments(&sidx, sidx_end);
    let duration = segments.iter().map(|s| s.duration).sum();
    let total_bytes = segments.iter().map(|s| s.size).sum();
    let bandwidth = average_bandwidth(total_bytes, duration, sidx.timescale);

    let mdia = &moov.trak[0].mdia;
    let codecs = &mdia.minf.stbl.stsd.codecs;
    let handler = mdia.hdlr.handler;
    let metadata = if handler == FourCC::new(b"vide") {
        let (codec, visual) = VideoCodec::from_codecs(codecs)?;
        let sample_duration = first_sample_duration(&moof, &moov);
        CmafMetadata::Video(VideoCmafMetadata {
            codec,
            width: visual.width as u32,
            height: visual.height as u32,
            frame_rate: frame_rate_ratio(sample_duration, sidx.timescale),
        })
    } else if handler == FourCC::new(b"soun") {
        let (codec, audio) = AudioCodec::from_codecs(codecs)?;
        CmafMetadata::Audio(AudioCmafMetadata {
            codec,
            sample_rate: audio.sample_rate.integer() as u32,
            channels: audio.channel_count,
            language: language_code(&mdia.mdhd),
        })
    } else if handler == FourCC::new(b"text") {
        let codec = TextCodec::from_codecs(codecs)?;
        CmafMetadata::Text(TextCmafMetadata {
            codec,
            language: optional_language_code(&mdia.mdhd),
        })
    } else {
        return Err(CoreError::Container(format!(
            "unrecognized media handler {handler}"
        )));
    };

    let header = CmafHeader {
        timescale: sidx.timescale,
        duration,
        bandwidth,
        earliest_presentation_time: sidx.earliest_presentation_time,
        init_segment: Segment {
            offset: 0,
            size: moov_end,
            duration: 0,
            duration_ms: 0,
        },
        segments,
    };
    Ok((header, metadata))
}

fn build_segments(sidx: &Sidx, sidx_end: u64) -> Vec<Segment> {
    let ts = sidx.timescale;
    let mut seg_offset = sidx_end + sidx.first_offset;
    let (mut acc_units, mut acc_ms) = (0u64, 0u64);
    let mut out = Vec::with_capacity(sidx.references.len());
    for r in &sidx.references {
        let duration = r.subsegment_duration as u64;
        acc_units += duration;
        let boundary_ms = crate::asset::units_to_ms(acc_units, ts);
        let duration_ms = boundary_ms - acc_ms;
        acc_ms = boundary_ms;
        out.push(Segment {
            offset: seg_offset,
            size: r.reference_size as u64,
            duration,
            duration_ms,
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
    if b == 0 { a } else { gcd(b, a % b) }
}

fn frame_rate_ratio(sample_duration: u32, timescale: u32) -> (u32, u32) {
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
    if lang.is_empty() { "und" } else { lang }
}

fn language_code(mdhd: &Mdhd) -> String {
    normalize_language(mdhd.language.as_str()).to_string()
}

/// The `mdhd` language as `Some(code)`, or `None` when the box leaves it empty.
fn optional_language_code(mdhd: &Mdhd) -> Option<String> {
    let lang = mdhd.language.as_str();
    (!lang.is_empty()).then(|| lang.to_string())
}

/// The media-agnostic result of parsing a CMAF track's header region: timing,
/// the init-segment location, and the (sub)segment map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmafHeader {
    /// Units per second for durations in this track.
    pub timescale: u32,
    /// Total presentation duration, in the track timescale.
    pub duration: u64,
    /// Average bitrate in bits/s, derived from the segment sizes and duration.
    pub bandwidth: u32,
    /// Presentation time of the first (sub)segment, in the track timescale.
    pub earliest_presentation_time: u64,
    /// Location of the init segment (`ftyp`+`moov`) within the track file.
    pub init_segment: Segment,
    /// The track's (sub)segments, in presentation order.
    pub segments: Vec<Segment>,
}

/// The parser's verdict on a CMAF track: which media type it is, plus the
/// per-type metadata read from the sample entry. Stored on the corresponding
/// [`VideoTrack`](crate::asset::VideoTrack) /
/// [`AudioTrack`](crate::asset::AudioTrack) as its `cmaf_metadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmafMetadata {
    /// A video track's metadata.
    Video(VideoCmafMetadata),
    /// An audio track's metadata.
    Audio(AudioCmafMetadata),
    /// A timed-text track's metadata.
    Text(TextCmafMetadata),
}

/// The media-specific fields parsed from a video track's sample entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoCmafMetadata {
    /// The decoded video codec and its RFC 6381 parameters.
    pub codec: VideoCodec,
    /// Visual width, in pixels.
    pub width: u32,
    /// Visual height, in pixels.
    pub height: u32,
    /// Frame rate as a (numerator, denominator) ratio, in frames per second.
    pub frame_rate: (u32, u32),
}

/// The media-specific fields parsed from an audio track's sample entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCmafMetadata {
    /// The decoded audio codec and its RFC 6381 parameters.
    pub codec: AudioCodec,
    /// Sampling rate, in Hz.
    pub sample_rate: u32,
    /// Number of audio channels (e.g. 2 for stereo, 6 for 5.1).
    pub channels: u16,
    /// ISO-639-2 language code (`"und"` when unspecified).
    pub language: String,
}

/// The media-specific fields parsed from a timed-text track's sample entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextCmafMetadata {
    /// The decoded text codec and its RFC 6381 parameters.
    pub codec: TextCodec,
    /// ISO-639-2 language code from the file's `mdhd` box, or `None` when the
    /// file leaves it unspecified.
    pub language: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn counting_reader_tracks_bytes_read() {
        let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut r = CountingReader::new(&data[..]);

        let mut first = [0u8; 3];
        r.read_exact(&mut first).await.unwrap();
        assert_eq!(r.count(), 3);

        let mut rest = Vec::new();
        r.read_to_end(&mut rest).await.unwrap();
        assert_eq!(r.count(), 8);
    }

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
        assert_eq!(frame_rate_ratio(1_600, 48_000), (30, 1));
    }

    #[test]
    fn frame_rate_is_zero_when_sample_duration_is_zero() {
        assert_eq!(frame_rate_ratio(0, 48_000), (0, 1));
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
    async fn probe_reads_a_packed_wvtt_text_track() {
        use opendal::services::Fs;

        use crate::text::subtitle::{Cue, Subtitle};

        let subtitle = Subtitle {
            language: "eng".to_string(),
            cues: vec![Cue {
                start_ms: 0,
                end_ms: 2000,
                text: "Hello".into(),
            }],
        };
        let segments = vec![Segment {
            offset: 0,
            size: 0,
            duration: 0,
            duration_ms: 4000,
        }];
        let windows = subtitle.expand(&segments);
        let bytes = crate::text::wvtt::pack(&subtitle.language, &windows, &segments).unwrap();

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("subs.mp4"), &bytes).unwrap();
        let op = Operator::new(Fs::default().root(dir.path().to_str().unwrap())).unwrap();

        let (h, m) = probe(&op, "subs.mp4").await.unwrap();
        assert!(!h.segments.is_empty(), "expected at least one segment");

        let CmafMetadata::Text(t) = m else {
            panic!("expected a text track, got {m:?}");
        };
        assert_eq!(t.codec, TextCodec::Wvtt);
        assert_eq!(t.language.as_deref(), Some("eng"));
    }

    #[tokio::test]
    async fn probe_returns_error_on_garbage_input_instead_of_panicking() {
        use opendal::services::Fs;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.mp4"), [0xAA_u8; 64]).unwrap();
        let op = Operator::new(Fs::default().root(dir.path().to_str().unwrap())).unwrap();

        let result = probe(&op, "bad.mp4").await;
        assert!(
            result.is_err(),
            "expected an error on garbage input, got Ok"
        );
    }

    fn reference(subsegment_duration: u32) -> mp4_atom::SegmentReference {
        mp4_atom::SegmentReference {
            reference_type: false,
            reference_size: 100,
            subsegment_duration,
            starts_with_sap: true,
            sap_type: 1,
            sap_delta_time: 0,
        }
    }

    #[test]
    fn build_segments_computes_drift_free_ms() {
        // timescale 3: three 1-unit segments are 1/3 s each. Independent
        // per-segment rounding gives 333+333+333 = 999 ms; cumulative-boundary
        // differencing gives 333+333+334 = 1000, matching the track total.
        let sidx = mp4_atom::Sidx {
            reference_id: 1,
            timescale: 3,
            earliest_presentation_time: 0,
            first_offset: 0,
            references: vec![reference(1), reference(1), reference(1)],
        };
        let segs = build_segments(&sidx, 0);
        let ms: Vec<u64> = segs.iter().map(|s| s.duration_ms).collect();
        assert_eq!(ms, vec![333, 333, 334]);
        assert_eq!(ms.iter().sum::<u64>(), 1000);
    }
}
