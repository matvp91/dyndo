pub mod wvtt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedTrack<S> {
    timescale: u32,
    fragments: Vec<TimedFragment<S>>,
}

impl<S> TimedTrack<S> {
    pub fn new(timescale: u32, fragments: Vec<TimedFragment<S>>) -> Self {
        Self {
            timescale,
            fragments,
        }
    }

    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    pub fn fragments(&self) -> &[TimedFragment<S>] {
        &self.fragments
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedFragment<S> {
    start_time: u64,
    samples: Vec<TimedSample<S>>,
}

impl<S> TimedFragment<S> {
    pub fn new(start_time: u64, samples: Vec<TimedSample<S>>) -> Self {
        Self {
            start_time,
            samples,
        }
    }

    pub fn start_time(&self) -> u64 {
        self.start_time
    }

    pub fn duration(&self) -> u64 {
        self.samples
            .iter()
            .map(|sample| u64::from(sample.duration))
            .sum()
    }

    pub fn samples(&self) -> &[TimedSample<S>] {
        &self.samples
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedSample<S> {
    duration: u32,
    value: S,
}

impl<S> TimedSample<S> {
    pub fn new(duration: u32, value: S) -> Self {
        Self { duration, value }
    }

    pub fn duration(&self) -> u32 {
        self.duration
    }

    pub fn value(&self) -> &S {
        &self.value
    }
}
