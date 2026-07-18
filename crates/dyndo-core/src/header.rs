//! A track file's parsed header: [`HeaderCmaf`] holds the values kept from
//! a CMAF box scan, [`HeaderRaw`] a raw file read whole. [`Header::read`]
//! reads one, dispatching on the file's [`Format`].

use opendal::Operator;

use crate::error::CoreError;
use crate::format::Format;
use crate::header_cmaf::HeaderCmaf;
use crate::header_raw::HeaderRaw;

/// A track file's parsed header, by format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Header {
    /// A CMAF track: the values kept from its header region.
    Cmaf(HeaderCmaf),
    /// A raw (non-CMAF) track, e.g. a plain `.vtt`: the whole file.
    Raw(HeaderRaw),
}

impl Header {
    /// Read the header of the track file at `path` through `op`,
    /// dispatching on its [`Format`].
    ///
    /// # Errors
    /// [`CoreError::UnsupportedFormat`] if `path`'s extension maps to no
    /// supported format; [`CoreError::Storage`] if the object cannot be
    /// read; [`CoreError::Container`] if a required box is missing or
    /// malformed.
    pub async fn read(op: &Operator, path: &str) -> Result<Header, CoreError> {
        match Format::from_path(path)? {
            Format::Cmaf => Ok(Header::Cmaf(HeaderCmaf::read(op, path).await?)),
            Format::Vtt => Ok(Header::Raw(HeaderRaw::read(op, path).await?)),
        }
    }
}
