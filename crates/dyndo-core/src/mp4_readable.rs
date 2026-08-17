use bytes::Bytes;
use futures_util::io::{AsyncRead, Cursor};
use relative_path::RelativePath;
use thiserror::Error;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::{
    mp4_box_reader::Mp4BoxReader,
    storage::{Storage, StorageError},
};

#[derive(Debug, Error)]
pub enum Mp4ReadableError {
    #[error("failed to access source: {0}")]
    Storage(#[from] StorageError),
    #[error("failed to read source: {0}")]
    Source(#[from] opendal::Error),
    #[error("failed to read MP4 atom: {0}")]
    Atom(#[from] mp4_atom::Error),
    #[error("invalid CMAF: {0}")]
    InvalidCmaf(String),
}

impl Mp4ReadableError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidCmaf(message.into())
    }
}

#[allow(async_fn_in_trait)]
pub trait Mp4Readable: Sized {
    type Error;
    type Output;

    async fn from_mp4_reader(
        reader: &mut Mp4BoxReader<impl tokio::io::AsyncRead + Unpin>,
    ) -> Result<Self::Output, Self::Error>;

    async fn from_async_reader(
        reader: &mut (impl AsyncRead + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        let mut reader = Mp4BoxReader::new(reader.compat());
        Self::from_mp4_reader(&mut reader).await
    }

    async fn from_bytes(bytes: Bytes) -> Result<Self::Output, Self::Error> {
        let mut reader = Cursor::new(bytes);
        Self::from_async_reader(&mut reader).await
    }

    async fn from_path(path: &RelativePath) -> Result<Self::Output, Self::Error>
    where
        Self::Error: From<Mp4ReadableError>,
    {
        let mut reader = source_reader(path).await?;

        Self::from_async_reader(&mut reader).await
    }
}

async fn source_reader(path: &RelativePath) -> Result<impl AsyncRead + Unpin, Mp4ReadableError> {
    let reader = Storage::source_op()?
        .reader(path.as_str())
        .await?
        .into_futures_async_read(..)
        .await?;

    Ok(reader)
}
