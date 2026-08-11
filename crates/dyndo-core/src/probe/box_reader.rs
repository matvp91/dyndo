use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::io::Cursor;
use mp4_atom::{AsyncReadAtom, AsyncReadFrom, Atom, Header as BoxHeader, Moof, Moov, Sidx};
use opendal::{FuturesAsyncReader, Operator};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt};

#[derive(Debug, thiserror::Error)]
pub enum BoxReaderError {
    #[error(transparent)]
    Storage(#[from] opendal::Error),
    #[error("track read failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed track container: {0}")]
    Parse(#[from] mp4_atom::Error),
    #[error("invalid track container: {0}")]
    Container(&'static str),
    #[error("invalid sidx reference")]
    InvalidSidxReference,
}

pub struct Boxes {
    pub moof: Moof,
    pub moov: Moov,
    pub sidx: Sidx,
    pub moov_end: u64,
    pub sidx_end: u64,
}

pub async fn scan(op: &Operator, path: &str) -> Result<Boxes, BoxReaderError> {
    let mut reader = reader(op, path).await?;
    let boxes = walk(&mut reader).await?;
    validate(&boxes)?;
    Ok(boxes)
}

pub async fn scan_bytes(bytes: Bytes) -> Result<Boxes, BoxReaderError> {
    let mut reader = CountingReader::new(Cursor::new(bytes).compat());
    let boxes = walk(&mut reader).await?;
    validate(&boxes)?;
    Ok(boxes)
}

fn validate(boxes: &Boxes) -> Result<(), BoxReaderError> {
    let Some(track) = boxes.moov.trak.first() else {
        return Err(BoxReaderError::Container("moov has no trak"));
    };
    if track.mdia.minf.stbl.stsd.codecs.is_empty() {
        return Err(BoxReaderError::Container("stsd has no sample entry"));
    }
    if boxes.sidx.timescale == 0 {
        return Err(BoxReaderError::Container("sidx timescale is zero"));
    }
    if boxes
        .sidx
        .references
        .iter()
        .any(|reference| reference.subsegment_duration == 0)
    {
        return Err(BoxReaderError::Container("sidx reference duration is zero"));
    }
    if boxes.sidx.references.iter().any(|reference| {
        reference.reference_type || !reference.starts_with_sap || reference.sap_type != 1
    }) {
        return Err(BoxReaderError::InvalidSidxReference);
    }
    Ok(())
}

async fn reader(
    op: &Operator,
    path: &str,
) -> Result<CountingReader<Compat<FuturesAsyncReader>>, BoxReaderError> {
    let inner = op
        .reader(path)
        .await?
        .into_futures_async_read(..)
        .await?
        .compat();
    Ok(CountingReader::new(inner))
}

async fn walk<R: AsyncRead + Unpin>(
    reader: &mut CountingReader<R>,
) -> Result<Boxes, BoxReaderError> {
    let mut moov: Option<Moov> = None;
    let mut moof: Option<Moof> = None;
    let mut sidx: Option<Sidx> = None;
    let mut moov_end = 0;
    let mut sidx_end = 0;

    while moov.is_none() || sidx.is_none() || moof.is_none() {
        let header = BoxHeader::read_from(&mut *reader).await?;
        let body_len = header
            .size
            .ok_or(BoxReaderError::Container("box has no size"))? as u64;

        if header.kind == Moov::KIND {
            moov = Some(parse(&header, &mut *reader).await?);
            moov_end = reader.count();
        } else if header.kind == Moof::KIND {
            moof = Some(parse(&header, &mut *reader).await?);
        } else if header.kind == Sidx::KIND {
            sidx = Some(parse(&header, &mut *reader).await?);
            sidx_end = reader.count();
        } else {
            skip(&mut *reader, body_len).await?;
        }
    }

    Ok(Boxes {
        moof: moof.ok_or(BoxReaderError::Container("missing moof"))?,
        moov: moov.ok_or(BoxReaderError::Container("missing moov"))?,
        sidx: sidx.ok_or(BoxReaderError::Container("missing sidx"))?,
        moov_end,
        sidx_end,
    })
}

async fn parse<A: AsyncReadAtom, R: AsyncRead + Unpin>(
    header: &BoxHeader,
    reader: &mut R,
) -> Result<A, BoxReaderError> {
    Ok(A::read_atom(header, reader).await?)
}

async fn skip<R: AsyncRead + Unpin>(reader: &mut R, len: u64) -> Result<(), BoxReaderError> {
    let copied = tokio::io::copy(&mut reader.take(len), &mut tokio::io::sink()).await?;
    if copied != len {
        return Err(BoxReaderError::Container("truncated box body"));
    }
    Ok(())
}

struct CountingReader<R> {
    inner: R,
    count: u64,
}

impl<R> CountingReader<R> {
    fn new(inner: R) -> Self {
        Self { inner, count: 0 }
    }

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
