use std::{
    pin::Pin,
    task::{Context, Poll},
};

use mp4_atom::{AsyncReadAtom, AsyncReadFrom, Atom, Error, Header};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

pub(crate) struct Mp4BoxReader<R> {
    reader: PositionReader<R>,
}

impl<R: AsyncRead + Unpin> Mp4BoxReader<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self {
            reader: PositionReader::new(reader),
        }
    }

    pub(crate) fn position(&self) -> u64 {
        self.reader.position()
    }

    pub(crate) async fn read_box<T: AsyncReadAtom + Atom>(&mut self) -> Result<T, Error> {
        loop {
            let header = Header::read_from(&mut self.reader).await?;
            let size = header.size.ok_or(Error::InvalidSize)?;

            if header.kind == T::KIND {
                return T::read_atom(&header, &mut self.reader).await;
            }

            skip(&mut self.reader, size).await?;
        }
    }
}

async fn skip(reader: &mut (impl AsyncRead + Unpin), size: usize) -> Result<(), Error> {
    let copied = tokio::io::copy(&mut reader.take(size as u64), &mut tokio::io::sink()).await?;
    if copied != size as u64 {
        return Err(Error::OutOfBounds);
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
