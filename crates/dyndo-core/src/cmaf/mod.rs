mod codec;
mod header;

pub use header::{read_header, TrackMeta};
// Part of the parser's complete output; consumed by the packaging server, not the CLI.
#[allow(unused_imports)]
pub use header::{ByteRange, CmafHeader, Segment};
