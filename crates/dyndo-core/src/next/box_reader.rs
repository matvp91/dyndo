//! Shared extraction of the `moov` and `sidx` boxes from a CMAF track.
//! [`scan`] stops as soon as both boxes have been read, before media payloads.

use std::pin::Pin;
use std::task::{Context, Poll};

use mp4_atom::{AsyncReadAtom, AsyncReadFrom, Atom, Header as BoxHeader, Moov, Sidx};
use opendal::{FuturesAsyncReader, Operator};
use relative_path::RelativePath;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt};

use super::error::Error;

/// The parsed `moov` and `sidx`, with their absolute end offsets.
pub struct Boxes {
    pub moov: Moov,
    pub sidx: Sidx,
    pub moov_end: u64,
    pub sidx_end: u64,
}

/// Scan the header region of the CMAF file at `path` through `op` into a
/// [`Boxes`].
///
/// # Errors
/// Returns an error if the track cannot be read or parsed, a required box is
/// missing or empty, or the segment-index timescale is zero.
pub async fn scan(op: &Operator, path: &RelativePath) -> Result<Boxes, Error> {
    let mut r = reader(op, path).await?;
    let boxes = walk(&mut r, path).await?;
    validate(&boxes, path)?;
    Ok(boxes)
}

/// Reject structural defects shared by all consumers.
fn validate(boxes: &Boxes, path: &RelativePath) -> Result<(), Error> {
    if boxes.sidx.timescale == 0 {
        return Err(invalid_track(path, "the segment-index timescale is zero"));
    }
    Ok(())
}

/// Open a byte-counting stream over the file at `path` through `op`.
async fn reader(
    op: &Operator,
    path: &RelativePath,
) -> Result<CountingReader<Compat<FuturesAsyncReader>>, Error> {
    let inner = op
        .reader(path.as_str())
        .await
        .map_err(|source| Error::OpenTrack {
            path: path.to_owned(),
            source,
        })?
        .into_futures_async_read(..)
        .await
        .map_err(|source| Error::OpenTrack {
            path: path.to_owned(),
            source,
        })?
        .compat();
    Ok(CountingReader::new(inner))
}

/// Walk `r`'s box structure, capturing `moov` and `sidx` and skipping every
/// other box. Stop as soon as both have been read.
async fn walk<R: AsyncRead + Unpin>(
    r: &mut CountingReader<R>,
    path: &RelativePath,
) -> Result<Boxes, Error> {
    let mut moov: Option<Moov> = None;
    let mut sidx: Option<Sidx> = None;
    let mut moov_end = 0u64;
    let mut sidx_end = 0u64;

    while moov.is_none() || sidx.is_none() {
        let header = BoxHeader::read_from(r)
            .await
            .map_err(|source| Error::ParseTrack {
                path: path.to_owned(),
                source,
            })?;
        let body_len = header
            .size
            .ok_or_else(|| invalid_track(path, "a box has no declared size"))?
            as u64;

        if header.kind == Moov::KIND {
            moov = Some(parse(&header, r, path).await?);
            moov_end = r.count();
        } else if header.kind == Sidx::KIND {
            sidx = Some(parse(&header, r, path).await?);
            sidx_end = r.count();
        } else {
            skip(r, body_len, path).await?;
        }
    }

    Ok(Boxes {
        moov: moov.ok_or_else(|| {
            invalid_track(
                path,
                "the movie box is missing before the first media fragment",
            )
        })?,
        sidx: sidx.ok_or_else(|| {
            invalid_track(
                path,
                "the segment index is missing before the first media fragment",
            )
        })?,
        moov_end,
        sidx_end,
    })
}

/// Parse the body of the box under `header` as atom `A`.
async fn parse<A: AsyncReadAtom, R: AsyncRead + Unpin>(
    header: &BoxHeader,
    r: &mut R,
    path: &RelativePath,
) -> Result<A, Error> {
    A::read_atom(header, r)
        .await
        .map_err(|source| Error::ParseTrack {
            path: path.to_owned(),
            source,
        })
}

/// Read and discard `len` bytes from `r`, erroring if the stream ends early.
async fn skip<R: AsyncRead + Unpin>(r: &mut R, len: u64, path: &RelativePath) -> Result<(), Error> {
    let copied = tokio::io::copy(&mut r.take(len), &mut tokio::io::sink())
        .await
        .map_err(|source| Error::ReadTrack {
            path: path.to_owned(),
            source,
        })?;
    if copied != len {
        return Err(invalid_track(path, "a box body is truncated"));
    }
    Ok(())
}

fn invalid_track(path: &RelativePath, reason: &str) -> Error {
    Error::InvalidTrack {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
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
