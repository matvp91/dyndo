//! The format-neutral subtitle model ([`Subtitle`], [`Cue`]) and its expansion
//! ([`Subtitle::expand`]) into one [`Subtitle`] per segment.

use std::collections::BTreeSet;

use crate::asset::Segment;

/// A parsed subtitle cue (format-neutral).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Start time, in milliseconds.
    pub start_ms: u64,
    /// End time, in milliseconds.
    pub end_ms: u64,
    /// Cue text, verbatim. An empty string denotes a gap-fill cue.
    pub text: String,
}

impl Cue {
    /// The cue's length, in milliseconds (`end_ms - start_ms`).
    pub fn duration(&self) -> u64 {
        self.end_ms - self.start_ms
    }
}

/// A subtitle track: a language plus its cues (in presentation order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtitle {
    /// ISO-639-2 language code, written into the track's `mdhd` by
    /// [`crate::text::wvtt::pack`].
    pub language: String,
    /// The cues, sorted by `(start_ms, end_ms)`.
    pub cues: Vec<Cue>,
}

impl Subtitle {
    /// Split into one `Subtitle` per segment, tiling each segment's window
    /// `[start, start + segment.duration_ms)` gaplessly. Windows are laid out
    /// consecutively from 0 (only `segment.duration_ms` is read). Overlapping
    /// cues share an interval; a window with no active cue yields a single
    /// empty-text gap cue. Cues outside the total timeline are clipped/dropped.
    /// Returns exactly `segments.len()` subtitles, each carrying `self.language`.
    pub fn expand(&self, segments: &[Segment]) -> Vec<Subtitle> {
        let mut out = Vec::with_capacity(segments.len());
        let mut start = 0u64;
        for seg in segments {
            let end = start + seg.duration_ms;
            out.push(Subtitle {
                language: self.language.clone(),
                cues: tile_window(&self.cues, start, end),
            });
            start = end;
        }
        out
    }
}

/// Tile `[w0, w1)` gaplessly from `cues`: split at every cue boundary strictly
/// inside the window; each subinterval becomes one cue per active cue (overlaps
/// share the interval) or a single empty gap cue.
fn tile_window(cues: &[Cue], w0: u64, w1: u64) -> Vec<Cue> {
    if w1 <= w0 {
        return Vec::new();
    }
    let mut bounds: BTreeSet<u64> = BTreeSet::from([w0, w1]);
    for c in cues {
        if c.start_ms > w0 && c.start_ms < w1 {
            bounds.insert(c.start_ms);
        }
        if c.end_ms > w0 && c.end_ms < w1 {
            bounds.insert(c.end_ms);
        }
    }
    let bounds: Vec<u64> = bounds.into_iter().collect();
    let mut tiled = Vec::new();
    for w in bounds.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        let active: Vec<&Cue> = cues
            .iter()
            .filter(|c| c.start_ms <= t0 && c.end_ms >= t1)
            .collect();
        if active.is_empty() {
            tiled.push(Cue {
                start_ms: t0,
                end_ms: t1,
                text: String::new(),
            });
        } else {
            for c in active {
                tiled.push(Cue {
                    start_ms: t0,
                    end_ms: t1,
                    text: c.text.clone(),
                });
            }
        }
    }
    tiled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(cues: Vec<Cue>) -> Subtitle {
        Subtitle {
            language: "und".into(),
            cues,
        }
    }
    fn cue(start_ms: u64, end_ms: u64, text: &str) -> Cue {
        Cue {
            start_ms,
            end_ms,
            text: text.into(),
        }
    }

    #[test]
    fn duration_is_end_minus_start() {
        assert_eq!(cue(1000, 3500, "hi").duration(), 2500);
    }

    fn seg_ms(duration_ms: u64) -> Segment {
        Segment {
            offset: 0,
            size: 0,
            duration: 0,
            duration_ms,
        }
    }

    #[test]
    fn expand_yields_one_subtitle_per_segment() {
        let out = sub(vec![cue(0, 1000, "A")]).expand(&[seg_ms(1000), seg_ms(1000)]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn expand_empty_segments_yields_empty() {
        assert!(sub(vec![cue(0, 1000, "A")]).expand(&[]).is_empty());
    }

    #[test]
    fn expand_propagates_language() {
        let out = sub(vec![cue(0, 1000, "A")]).expand(&[seg_ms(1000)]);
        assert_eq!(out[0].language, "und");
    }

    #[test]
    fn expand_gap_fills_between_sequential_cues() {
        // one 3000ms segment: [0,1000)=A, [1000,2000)=gap, [2000,3000)=B
        let out = sub(vec![cue(0, 1000, "A"), cue(2000, 3000, "B")]).expand(&[seg_ms(3000)]);
        assert_eq!(out.len(), 1);
        let cues = &out[0].cues;
        assert_eq!(cues.len(), 3);
        assert_eq!(cues[0].text, "A");
        assert_eq!(cues[1].text, "");
        assert_eq!((cues[1].start_ms, cues[1].end_ms), (1000, 2000));
        assert_eq!(cues[2].text, "B");
    }

    #[test]
    fn expand_overlapping_cues_share_an_interval() {
        let out = sub(vec![cue(0, 5000, "A"), cue(3000, 8000, "B")]).expand(&[seg_ms(8000)]);
        let cues = &out[0].cues;
        // [0,3000)=A, [3000,5000)=A+B, [5000,8000)=B → 4 tiled cues
        assert_eq!(cues.len(), 4);
        let overlap: Vec<&str> = cues
            .iter()
            .filter(|c| c.start_ms == 3000 && c.end_ms == 5000)
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(overlap.len(), 2);
        assert!(overlap.contains(&"A") && overlap.contains(&"B"));
    }

    #[test]
    fn expand_splits_a_cue_across_segment_boundaries() {
        // cue [2000,6000) over two 4000ms segments → windows [0,4000), [4000,8000)
        let out = sub(vec![cue(2000, 6000, "A")]).expand(&[seg_ms(4000), seg_ms(4000)]);
        assert_eq!(out.len(), 2);
        // seg 0: gap[0,2000), A[2000,4000)
        assert_eq!(out[0].cues.len(), 2);
        assert_eq!(out[0].cues[0].text, "");
        assert_eq!(out[0].cues[1].text, "A");
        assert_eq!(out[0].cues[1].end_ms, 4000);
        // seg 1: A[4000,6000), gap[6000,8000)
        assert_eq!(out[1].cues.len(), 2);
        assert_eq!(out[1].cues[0].text, "A");
        assert_eq!(
            (out[1].cues[0].start_ms, out[1].cues[0].end_ms),
            (4000, 6000)
        );
        assert_eq!(out[1].cues[1].text, "");
        assert_eq!(
            (out[1].cues[1].start_ms, out[1].cues[1].end_ms),
            (6000, 8000)
        );
    }

    #[test]
    fn expand_empty_window_is_one_gap_cue() {
        let out = sub(vec![]).expand(&[seg_ms(2000)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].cues.len(), 1);
        assert_eq!(out[0].cues[0].text, "");
        assert_eq!((out[0].cues[0].start_ms, out[0].cues[0].end_ms), (0, 2000));
    }
}
