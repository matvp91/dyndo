use mp4_atom::{FourCC, Mdhd, Moof, Moov};

use super::boxes::scan_header_boxes;
use super::codec;
use super::segment::{segments, Segment};
use super::stream::{AudioStream, Stream, VideoStream};
use crate::error::{Error, Result};
use crate::storage::Source;

/// A parsed CMAF header: the fields common to every track, plus a `stream`
/// discriminant carrying the video- or audio-specific fields.
#[derive(Debug, Clone, PartialEq)]
pub struct CmafHeader {
    pub timescale: u32,
    pub duration: u64,
    /// Average bitrate in bits/s, derived from the segment sizes and duration.
    pub bandwidth: u32,
    pub earliest_presentation_time: u64,
    pub init_segment: Segment,
    pub segments: Vec<Segment>,
    pub stream: Stream,
}

fn language_string(mdhd: &Mdhd) -> Option<String> {
    match mdhd.language.as_str() {
        "" | "und" => None,
        s => Some(s.to_string()),
    }
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

/// Parse the CMAF header of a single-track fragmented MP4, scanning only the
/// `ftyp`/`moov`/`sidx`/first-`moof` boxes — `mdat` is never fetched.
///
/// # Errors
/// - [`Error::NotSingleTrack`] if the `moov` has zero or multiple tracks.
/// - [`Error::MissingMoov`] or [`Error::MissingSidx`] if either box is absent.
/// - [`Error::UnsupportedCodec`] for a handler that is neither video nor audio.
/// - [`Error::MalformedBox`] if a box header or body fails to parse.
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

    let segments = segments(&scanned.sidx, scanned.sidx_end);
    let duration: u64 = segments.iter().map(|s| s.duration).sum();
    let total_bytes: u64 = segments.iter().map(|s| s.size).sum();
    let bandwidth = average_bandwidth(total_bytes, duration, scanned.sidx.timescale);

    let mdia = &trak.mdia;
    let handler = mdia.hdlr.handler;
    let codecs = &mdia.minf.stbl.stsd.codecs;
    // The init segment (ftyp+moov) carries no media samples, hence no duration.
    let init_segment = Segment {
        offset: 0,
        size: scanned.moov_end,
        duration: 0,
    };

    let stream = if handler == FourCC::new(b"vide") {
        let (codec, visual) = codec::video_codec(codecs, path)?;
        let sample_duration = first_sample_duration(&scanned.first_moof, &scanned.moov);
        // frame_rate divides the moof sample duration by the sidx timescale; conformant
        // CMAF has sidx.timescale == mdhd (media) timescale, so this is exact.
        let frame_rate = frame_rate(sample_duration, scanned.sidx.timescale);
        Stream::Video(VideoStream {
            codec,
            width: visual.width as u32,
            height: visual.height as u32,
            frame_rate,
        })
    } else if handler == FourCC::new(b"soun") {
        let (codec, audio) = codec::audio_codec(codecs, path)?;
        Stream::Audio(AudioStream {
            codec,
            sample_rate: audio.sample_rate.integer() as u32,
            channels: audio.channel_count,
            language: language_string(&mdia.mdhd),
        })
    } else {
        return Err(Error::UnsupportedCodec {
            path: path.into(),
            codec: format!("{:?}", handler),
        });
    };

    Ok(CmafHeader {
        timescale: scanned.sidx.timescale,
        duration,
        bandwidth,
        earliest_presentation_time: scanned.sidx.earliest_presentation_time,
        init_segment,
        segments,
        stream,
    })
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
        assert_eq!(h.timescale, 90000);
        assert_eq!(h.duration, 123328800);
        assert_eq!(h.bandwidth, 4807228);
        assert_eq!(h.earliest_presentation_time, 0);
        assert_eq!(h.segments.len(), 715);
        assert_eq!(h.init_segment.offset, 0);
        assert_eq!(h.init_segment.size, 766);
        assert_eq!(h.segments[0].offset, 9386);
        assert_eq!(h.segments[0].size, 1495550);
        assert_eq!(h.segments[0].duration, 172800);
        assert_eq!(h.stream.fourcc(), "avc1");
        assert_eq!(h.stream.rfc6381(), "avc1.640028");
        assert_eq!(h.stream.language(), None);
        let Stream::Video(v) = h.stream else {
            panic!("expected video");
        };
        assert_eq!((v.width, v.height), (1920, 1080));
        assert_eq!(v.frame_rate, (25, 1));
    }

    #[tokio::test]
    async fn reads_audio_header() {
        let src = fixture("audio_aac_nl_2.mp4");
        let h = read_header(&src, "audio_aac_nl_2.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.duration, 65775616);
        assert_eq!(h.bandwidth, 196918);
        assert_eq!(h.earliest_presentation_time, 0);
        assert_eq!(h.segments.len(), 715);
        assert_eq!(h.init_segment.size, 662);
        assert_eq!(h.segments[0].offset, 9282);
        assert_eq!(h.segments[0].size, 48530);
        assert!(h.segments[0].duration > 0);
        assert_eq!(h.stream.fourcc(), "mp4a");
        assert_eq!(h.stream.rfc6381(), "mp4a.40.2");
        assert_eq!(h.stream.language(), Some("nld"));
        let Stream::Audio(a) = h.stream else {
            panic!("expected audio");
        };
        assert_eq!(a.sample_rate, 48000);
        assert_eq!(a.channels, 2);
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
        assert_eq!(h.init_segment.size, 783);
        assert_eq!(h.segments[0].offset, 847);
        assert_eq!(h.segments[0].size, 8342);
        assert_eq!(h.segments[0].duration, 12800);
        assert_eq!(h.stream.fourcc(), "av01");
        let Stream::Video(v) = h.stream else {
            panic!("expected video");
        };
        assert_eq!((v.width, v.height), (320, 240));
    }

    #[tokio::test]
    async fn reads_ac3_audio_header() {
        let src = fixture("audio_ac3_1.mp4");
        let h = read_header(&src, "audio_ac3_1.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.segments.len(), 3);
        assert_eq!(h.init_segment.size, 725);
        assert_eq!(h.segments[0].offset, 801);
        assert_eq!(h.segments[0].size, 24684);
        assert_eq!(h.stream.fourcc(), "ac-3");
        let Stream::Audio(a) = h.stream else {
            panic!("expected audio");
        };
        assert_eq!(a.sample_rate, 48000);
        assert_eq!(a.channels, 1);
    }

    #[tokio::test]
    async fn reads_ec3_audio_header() {
        let src = fixture("audio_ec3_1.mp4");
        let h = read_header(&src, "audio_ec3_1.mp4").await.unwrap();
        assert_eq!(h.timescale, 48000);
        assert_eq!(h.segments.len(), 3);
        assert_eq!(h.init_segment.size, 727);
        assert_eq!(h.segments[0].offset, 803);
        assert_eq!(h.segments[0].size, 24684);
        assert_eq!(h.stream.fourcc(), "ec-3");
        let Stream::Audio(a) = h.stream else {
            panic!("expected audio");
        };
        assert_eq!(a.sample_rate, 48000);
        assert_eq!(a.channels, 1);
    }
}
