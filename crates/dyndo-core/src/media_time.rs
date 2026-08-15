use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaTime {
    value: u64,
    timescale: u32,
}

impl MediaTime {
    pub fn new(value: u64, timescale: u32) -> Self {
        assert!(timescale > 0);

        Self { value, timescale }
    }

    pub fn value(&self) -> u64 {
        self.value
    }

    pub fn timescale(&self) -> u32 {
        self.timescale
    }

    pub fn as_millis(&self) -> u64 {
        self.value * 1_000 / u64::from(self.timescale)
    }

    pub fn as_duration(&self) -> Duration {
        Duration::from_millis(self.as_millis())
    }
}
