//! WebVTT parsing and packing into a CMAF `wvtt` track (ISO/IEC 14496-30).

mod error;
mod vtt;
mod vtt_cue;

pub use error::CoreTextError;
pub use vtt::{parse, WebVtt};
pub use vtt_cue::VttCue;
