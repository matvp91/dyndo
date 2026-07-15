//! WebVTT parsing and packing into a CMAF `wvtt` track (ISO/IEC 14496-30).

mod error;
mod subtitle;
mod vtt;
pub mod wvtt;

pub use error::CoreTextError;
pub use subtitle::{chunk, Cue, Subtitle, SubtitleChunk};
pub use vtt::parse;
