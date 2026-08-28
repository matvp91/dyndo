use std::{
    pin::Pin,
    task::{Context, Poll},
};

use mp4_atom::{AsyncReadAtom, AsyncReadFrom, Atom, Error as AtomError, Header};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

use crate::mp4_readable::Mp4ReadableError;

/// An asynchronous MP4 box reader that tracks its position in the source.
pub struct Mp4BoxReader<R> {
    reader: PositionReader<R>,
}

impl<R: AsyncRead + Unpin> Mp4BoxReader<R> {
    /// Creates an MP4 box reader over `reader`.
    pub fn new(reader: R) -> Self {
        Self {
            reader: PositionReader::new(reader),
        }
    }

    /// Returns the current byte offset in the source.
    pub fn position(&self) -> u64 {
        self.reader.position()
    }

    /// Reads the next box of type `T`, skipping boxes of other types.
    pub async fn read_box<T: AsyncReadAtom + Atom>(&mut self) -> Result<T, Mp4ReadableError> {
        loop {
            let header = Header::read_from(&mut self.reader).await?;
            let size = header.size.ok_or(AtomError::InvalidSize)?;

            if header.kind == T::KIND {
                return Ok(T::read_atom(&header, &mut self.reader).await?);
            }

            skip(&mut self.reader, size).await?;
        }
    }
}

async fn skip(reader: &mut (impl AsyncRead + Unpin), size: usize) -> Result<(), AtomError> {
    let copied = tokio::io::copy(&mut reader.take(size as u64), &mut tokio::io::sink()).await?;
    if copied != size as u64 {
        return Err(AtomError::OutOfBounds);
    }

    Ok(())
}

struct PositionReader<R> {
    inner: R,
    position: u64,
}

impl<R> PositionReader<R> {
    fn new(inner: R) -> Self {
        Self { inner, position: 0 }
    }

    fn position(&self) -> u64 {
        self.position
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for PositionReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let poll = Pin::new(&mut this.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            this.position += (buf.filled().len() - before) as u64;
        }
        poll
    }
}
