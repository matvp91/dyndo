//! Shared box extraction over a CMAF track's header region (`moov`, `sidx`,
//! and the first `moof`). [`scan`] opens a byte-counting stream over the
//! file and walks its box structure into a [`Boxes`], the intermediate
//! `Header::read` and `Metadata::read` fold from. `mdat` is never fetched.

use std::pin::Pin;
use std::task::{Context, Poll};

use mp4_atom::{AsyncReadAtom, AsyncReadFrom, Atom, Header as BoxHeader, Moof, Moov, Sidx};
use opendal::{FuturesAsyncReader, Operator};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt};

use crate::error::CoreError;

/// The parsed header-region boxes of a CMAF track file, with the absolute
/// byte offsets just past the `moov` and `sidx` boxes.
pub struct Boxes {
    pub moov: Moov,
    pub sidx: Sidx,
    pub moof: Moof,
    pub moov_end: u64,
    pub sidx_end: u64,
}

/// Scan the header region of the CMAF file at `path` through `op` into a
/// [`Boxes`].
///
/// # Errors
/// [`CoreError::Storage`] if the object cannot be read;
/// [`CoreError::Container`] if a required box is missing or malformed.
pub async fn scan(op: &Operator, path: &str) -> Result<Boxes, CoreError> {
    let mut r = reader(op, path).await?;
    walk(&mut r).await
}

/// Open a byte-counting stream over the file at `path` through `op`.
async fn reader(
    op: &Operator,
    path: &str,
) -> Result<CountingReader<Compat<FuturesAsyncReader>>, CoreError> {
    let inner = op
        .reader(path)
        .await?
        .into_futures_async_read(..)
        .await?
        .compat();
    Ok(CountingReader::new(inner))
}

/// Walk `r`'s box structure, capturing the `moov`, `sidx`, and first `moof`
/// boxes as they pass and skipping every other box. Stops at the first
/// `moof`, so `mdat` is never touched.
async fn walk<R: AsyncRead + Unpin>(r: &mut CountingReader<R>) -> Result<Boxes, CoreError> {
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut moof: Option<Moof> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    while moov.is_none() || sidx.is_none() || moof.is_none() {
        let header = BoxHeader::read_from(r)
            .await
            .map_err(|e| CoreError::Container(e.to_string()))?;
        let body_len = header
            .size
            .ok_or_else(|| CoreError::Container("box has no size".into()))?
            as u64;

        if header.kind == Moov::KIND {
            moov = Some(parse(&header, r).await?);
            moov_end = r.count();
        } else if header.kind == Sidx::KIND {
            sidx = Some(parse(&header, r).await?);
            sidx_end = r.count();
        } else if header.kind == Moof::KIND {
            // The first `moof` ends the header region; `mdat` follows it.
            moof = Some(parse(&header, r).await?);
            break;
        } else {
            skip(r, body_len).await?;
        }
    }

    Ok(Boxes {
        moov: moov.ok_or_else(|| CoreError::Container("missing moov before first moof".into()))?,
        sidx: sidx.ok_or_else(|| CoreError::Container("missing sidx before first moof".into()))?,
        moof: moof.ok_or_else(|| CoreError::Container("missing moof".into()))?,
        moov_end,
        sidx_end,
    })
}

/// Parse the body of the box under `header` as atom `A`.
async fn parse<A: AsyncReadAtom, R: AsyncRead + Unpin>(
    header: &BoxHeader,
    r: &mut R,
) -> Result<A, CoreError> {
    A::read_atom(header, r)
        .await
        .map_err(|e| CoreError::Container(e.to_string()))
}

/// Read and discard `len` bytes from `r`, erroring if the stream ends early.
async fn skip<R: AsyncRead + Unpin>(r: &mut R, len: u64) -> Result<(), CoreError> {
    let copied = tokio::io::copy(&mut r.take(len), &mut tokio::io::sink())
        .await
        .map_err(|e| CoreError::Container(e.to_string()))?;
    if copied != len {
        return Err(CoreError::Container("truncated box body".into()));
    }
    Ok(())
}

/// An [`AsyncRead`] that tallies every byte read through it, so the streamed
/// scan can record absolute box offsets (`moov`/`sidx` end) without seeking.
struct CountingReader<R> {
    inner: R,
    count: u64,
}

impl<R> CountingReader<R> {
    fn new(inner: R) -> Self {
        Self { inner, count: 0 }
    }

    /// Total bytes read through this reader so far.
    fn count(&self) -> u64 {
        self.count
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for CountingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            self.count += (buf.filled().len() - before) as u64;
        }
        poll
    }
}
