//! One text sample: the cues on screen over an interval, as the boxes ISO/IEC
//! 14496-30 fills a sample with.
//!
//! [`encode`] and [`decode`] are inverses.

use mp4_atom::{Any, Atom, BufMut, DecodeMaybe, Encode};

use super::UnpackError;
use super::atom::{Payl, Vttc, Vtte};
use crate::fragmenter::Sample;
use crate::subtitle::Cue;

/// Write the cues on screen over a sample, each a `vttc` carrying its text. An
/// interval showing nothing is a lone `vtte`, which the format still spends a
/// sample on.
pub(super) fn encode<B: BufMut>(sample: &Sample, buf: &mut B) -> mp4_atom::Result<()> {
    if sample.cues.is_empty() {
        return Vtte.encode(buf);
    }

    for cue in &sample.cues {
        Vttc {
            payl: Payl {
                text: cue.text.clone(),
            },
        }
        .encode(buf)?;
    }

    Ok(())
}

/// The cues a sample carries, one per `vttc` on screen over it. A `vtte` carries
/// none — the box the format spends on an interval showing nothing.
///
/// Each cue spans the sample, since a `vttc` records what is on screen without
/// saying for how long. The authored span is recoverable only by merging the samples
/// a cue runs across, which [`merge`](crate::fragmenter::merge) does.
///
/// # Errors
///
/// [`UnpackError::Atom`] if a cue box fails to decode.
pub(super) fn decode(sample: &[u8], start: u32, end: u32) -> Result<Vec<Cue>, UnpackError> {
    let mut cues = Vec::new();
    let mut buf = sample;

    while let Some(atom) = Any::decode_maybe(&mut buf)? {
        // The cue boxes are ours rather than mp4-atom's, so they arrive unknown.
        let Any::Unknown(kind, body) = atom else {
            continue;
        };
        if kind == Vttc::KIND {
            let vttc = Vttc::decode_body(&mut body.as_slice())?;
            cues.push(Cue {
                start,
                end,
                text: vttc.payl.text,
            });
        }
    }

    Ok(cues)
}
