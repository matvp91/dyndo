//! S3-backed source. Stubbed until the S3 SDK is wired in.

use crate::error::{Error, Result};
use crate::storage::Source;

pub struct S3Source {
    pub bucket: String,
    pub key: String,
}

impl Source for S3Source {
    async fn size(&self) -> Result<u64> {
        Err(Error::Backend("s3 unimplemented".into()))
    }

    async fn read_at(&self, _offset: u64, _len: usize) -> Result<Vec<u8>> {
        Err(Error::Backend("s3 unimplemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_backend_error() {
        let src = S3Source {
            bucket: "some-bucket".into(),
            key: "some/key".into(),
        };

        match src.size().await {
            Err(Error::Backend(_)) => {}
            other => panic!("expected Err(Error::Backend(_)), got {other:?}"),
        }

        match src.read_at(0, 1).await {
            Err(Error::Backend(_)) => {}
            other => panic!("expected Err(Error::Backend(_)), got {other:?}"),
        }
    }
}
