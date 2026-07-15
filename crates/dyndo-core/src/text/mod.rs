//! WebVTT parsing and packing into a CMAF `wvtt` track (ISO/IEC 14496-30).

mod error;
mod subtitle;
mod subtitle_chunk;
mod vtt;
pub mod wvtt;

pub use error::CoreTextError;
pub use subtitle::{Cue, Subtitle};
pub use subtitle_chunk::{chunk, SubtitleChunk};
pub use vtt::parse;
