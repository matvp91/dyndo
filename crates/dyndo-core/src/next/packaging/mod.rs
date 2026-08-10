mod format;
mod initialization;
mod media_segment;
mod packager;
mod unpackager;

pub mod wvtt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSegment<S> {
    base_decode_time: u64,
    samples: Vec<Sample<S>>,
}

impl<S> MediaSegment<S> {
    pub fn new(base_decode_time: u64, samples: Vec<Sample<S>>) -> Self {
        Self {
            base_decode_time,
            samples,
        }
    }

    pub fn base_decode_time(&self) -> u64 {
        self.base_decode_time
    }

    pub fn duration(&self) -> u64 {
        self.samples
            .iter()
            .map(|sample| u64::from(sample.duration))
            .sum()
    }

    pub fn samples(&self) -> &[Sample<S>] {
        &self.samples
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample<S> {
    duration: u32,
    payload: S,
}

impl<S> Sample<S> {
    pub fn new(duration: u32, payload: S) -> Self {
        Self { duration, payload }
    }

    pub fn duration(&self) -> u32 {
        self.duration
    }

    pub fn payload(&self) -> &S {
        &self.payload
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpackagedMedia<P> {
    timescale: u32,
    segments: Vec<MediaSegment<P>>,
}

impl<P> UnpackagedMedia<P> {
    pub fn new(timescale: u32, segments: Vec<MediaSegment<P>>) -> Self {
        Self {
            timescale,
            segments,
        }
    }

    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    pub fn segments(&self) -> &[MediaSegment<P>] {
        &self.segments
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("track ID must not be zero")]
    InvalidTrackId,
    #[error("timescale must not be zero")]
    InvalidTimescale,
    #[error("media covers no time")]
    Empty,
    #[error("a sample is too large")]
    SampleTooLarge,
    #[error("a media segment is too large")]
    MediaSegmentTooLarge,
    #[error("a media segment duration overflows")]
    DurationOverflow,
    #[error("media contains too many media segments")]
    TooManyMediaSegments,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum UnpackageError {
    #[error("media carries no timescale")]
    MissingTimescale,
    #[error("media segment carries no base decode time")]
    MissingBaseTime,
    #[error("media segment carries no sample durations")]
    MissingSampleTiming,
    #[error("sample data overruns the media segment")]
    SampleOutOfRange,
    #[error("a movie fragment and its media data do not pair up")]
    UnpairedMediaSegment,
    #[error(transparent)]
    Atom(#[from] mp4_atom::Error),
}
