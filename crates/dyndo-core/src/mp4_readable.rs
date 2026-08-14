use bytes::Bytes;
use futures_util::io::{AsyncRead, Cursor};

#[allow(async_fn_in_trait)]
pub trait Mp4Readable: Sized {
    type Error;

    async fn from_reader(reader: &mut (impl AsyncRead + Unpin)) -> Result<Self, Self::Error>;

    async fn from_bytes(bytes: Bytes) -> Result<Self, Self::Error> {
        let mut reader = Cursor::new(bytes);
        Self::from_reader(&mut reader).await
    }
}
