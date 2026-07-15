//! WebVTT parsing and packing into a CMAF `wvtt` track (ISO/IEC 14496-30).

mod error;
mod subtitle;
mod vtt;
mod vtt_cue;
pub mod wvtt;

pub use error::CoreTextError;
pub use subtitle::{Cue, Subtitle};
pub use vtt::{parse, WebVtt};
pub use vtt_cue::VttCue;
