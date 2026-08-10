use super::{Cue, Subtitle};
use crate::packaging::wvtt::{WvttPackager, WvttSample, WvttUnpackager};
use crate::packaging::{MediaSegment, PackageError, Sample, UnpackageError};
use crate::time::Time;

const TIMESCALE: u32 = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum WvttParseError {
    #[error(transparent)]
    Unpackage(#[from] UnpackageError),
    #[error("WVTT timescale must not be zero")]
    InvalidTimescale,
    #[error("subtitle timestamp {0}ms overflows")]
    TimeOverflow(u64),
}

impl Subtitle {
    pub fn from_wvtt(bytes: &[u8]) -> Result<Self, WvttParseError> {
        let media = WvttUnpackager::new().unpackage(bytes)?;
        if media.timescale() == 0 {
            return Err(WvttParseError::InvalidTimescale);
        }

        let mut cues: Vec<Cue> = Vec::new();
        let mut open: Vec<usize> = Vec::new();

        for segment in media.segments() {
            let mut start = segment.base_decode_time();
            for sample in segment.samples() {
                let end = start
                    .checked_add(u64::from(sample.duration()))
                    .ok_or(WvttParseError::TimeOverflow(start))?;
                let start_ms = timestamp(start, media.timescale())?;
                let end_ms = timestamp(end, media.timescale())?;
                let mut still_open = Vec::with_capacity(sample.payload().cues().len());

                for text in sample.payload().cues() {
                    let continued = open
                        .iter()
                        .copied()
                        .find(|&index| cues[index].end == start_ms && cues[index].text == *text);
                    match continued {
                        Some(index) => {
                            cues[index].end = end_ms;
                            still_open.push(index);
                        }
                        None => {
                            cues.push(Cue {
                                start: start_ms,
                                end: end_ms,
                                text: text.clone(),
                            });
                            still_open.push(cues.len() - 1);
                        }
                    }
                }

                open = still_open;
                start = end;
            }
        }

        cues.sort_by_key(|cue| (cue.start, cue.end));
        Ok(Self { cues })
    }

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

fn timestamp(time: u64, timescale: u32) -> Result<u32, WvttParseError> {
    let milliseconds = Time::milliseconds(time, timescale);
    u32::try_from(milliseconds).map_err(|_| WvttParseError::TimeOverflow(milliseconds))
}
