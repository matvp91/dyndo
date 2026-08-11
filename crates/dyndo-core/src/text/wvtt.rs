use super::Subtitle;
use crate::packaging::wvtt::{WvttPackager, WvttSample};
use crate::packaging::{MediaSegment, PackageError, Sample};

const TIMESCALE: u32 = 1_000;

impl Subtitle {
    pub fn to_wvtt(
        &self,
        segment_duration: u32,
        boundaries: &[u32],
    ) -> Result<Vec<u8>, PackageError> {
        let segments = self
            .segments(segment_duration, boundaries)
            .into_iter()
            .map(|segment| {
                let samples = segment
                    .samples()
                    .iter()
                    .map(|sample| {
                        let cues = sample.cues().iter().map(|cue| cue.text.clone()).collect();
                        Sample::new(sample.duration(), WvttSample::new(cues))
                    })
                    .collect();
                MediaSegment::new(u64::from(segment.start()), samples)
            })
            .collect::<Vec<_>>();

        WvttPackager::new(TIMESCALE).package(&segments)
    }
}
