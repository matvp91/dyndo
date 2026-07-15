//! The format-neutral subtitle model ([`Subtitle`], [`Cue`]) and its chunking
//! into [`SubtitleChunk`]s.

use std::collections::BTreeSet;

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

/// One chunk of a subtitle timeline: the fully-tiled cues within one window,
/// later encoded as one CMAF segment. Cues tile the window contiguously (no
/// gaps, no overlaps beyond identical `[start, end]` intervals); a gap is a
/// [`Cue`] with empty `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleChunk {
    /// Tiled cues for this chunk, in presentation order.
    pub cues: Vec<Cue>,
}

/// Tile `subtitle.cues` and group them into `chunk_duration_ms` windows.
///
/// A sweep-line over the sorted, unique set of boundaries — every cue
/// `start_ms`/`end_ms`, every window edge `k · chunk_duration_ms` in
/// `(0, track_end)`, plus `0` and `track_end` (= max cue `end_ms`) — yields a
/// gapless, non-overlapping tiling: each `[t0, t1)` interval becomes one cue per
/// active cue (overlaps ⇒ several cues sharing `[t0, t1]`), or a single empty
/// gap cue when none is active. Tiled cues are grouped into chunks by
/// `start_ms / chunk_duration_ms`. Returns an empty `Vec` when there are no cues.
pub fn chunk(subtitle: &Subtitle, chunk_duration_ms: u64) -> Vec<SubtitleChunk> {
    let cues = &subtitle.cues;
    if cues.is_empty() {
        return Vec::new();
    }
    let track_end = cues.iter().map(|c| c.end_ms).max().unwrap_or(0);

    let mut bounds: BTreeSet<u64> = BTreeSet::new();
    bounds.insert(0);
    bounds.insert(track_end);
    for c in cues {
        bounds.insert(c.start_ms);
        bounds.insert(c.end_ms);
    }
    if chunk_duration_ms > 0 {
        let mut t = chunk_duration_ms;
        while t < track_end {
            bounds.insert(t);
            t += chunk_duration_ms;
        }
    }
    let bounds: Vec<u64> = bounds.into_iter().collect();

    // Tile each [t0, t1) interval; a gap becomes one empty-text cue.
    let mut tiled: Vec<Cue> = Vec::new();
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

    // Group into windows; no tiled cue crosses a window boundary.
    let window = chunk_duration_ms.max(1);
    let mut chunks: Vec<SubtitleChunk> = Vec::new();
    let mut cur_idx: Option<u64> = None;
    for cue in tiled {
        let idx = cue.start_ms / window;
        if Some(idx) != cur_idx {
            chunks.push(SubtitleChunk { cues: Vec::new() });
            cur_idx = Some(idx);
        }
        chunks.last_mut().expect("just pushed").cues.push(cue);
    }
    chunks
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

    #[test]
    fn empty_subtitle_yields_no_chunks() {
        assert!(chunk(&sub(vec![]), 4000).is_empty());
    }

    #[test]
    fn sequential_cues_get_gap_fill() {
        let chunks = chunk(&sub(vec![cue(0, 1000, "A"), cue(2000, 3000, "B")]), 100_000);
        assert_eq!(chunks.len(), 1);
        let cues = &chunks[0].cues;
        // [0,1000)=A, [1000,2000)=gap, [2000,3000)=B
        assert_eq!(cues.len(), 3);
        assert_eq!(cues[0].text, "A");
        assert_eq!(cues[1].text, "");
        assert_eq!(cues[1].start_ms, 1000);
        assert_eq!(cues[1].end_ms, 2000);
        assert_eq!(cues[2].text, "B");
    }

    #[test]
    fn overlapping_cues_share_an_interval() {
        let chunks = chunk(&sub(vec![cue(0, 5000, "A"), cue(3000, 8000, "B")]), 100_000);
        assert_eq!(chunks.len(), 1);
        let cues = &chunks[0].cues;
        // [0,3000)=A, [3000,5000)=A+B, [5000,8000)=B => 4 tiled cues
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
    fn cue_crossing_window_boundary_splits_into_two_chunks() {
        let chunks = chunk(&sub(vec![cue(2000, 6000, "A")]), 4000);
        // windows [0,4000) and [4000,8000); track_end=6000
        // chunk0: gap[0,2000), A[2000,4000) ; chunk1: A[4000,6000)
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].cues.len(), 2);
        assert_eq!(chunks[0].cues[0].text, "");
        assert_eq!(chunks[0].cues[1].text, "A");
        assert_eq!(chunks[0].cues[1].end_ms, 4000);
        assert_eq!(chunks[1].cues.len(), 1);
        assert_eq!(chunks[1].cues[0].text, "A");
        assert_eq!(chunks[1].cues[0].start_ms, 4000);
        assert_eq!(chunks[1].cues[0].end_ms, 6000);
    }
}
