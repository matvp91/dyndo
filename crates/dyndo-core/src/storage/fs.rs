//! Local filesystem source, backed by `tokio::fs`.

use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::error::{Error, Result};
use crate::storage::Source;

pub struct LocalFile {
    path: PathBuf,
}

impl LocalFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    fn io_err(&self, source: std::io::Error) -> Error {
        Error::Io {
            path: self.path.display().to_string(),
            source,
        }
    }
}

impl Source for LocalFile {
    async fn size(&self) -> Result<u64> {
        let meta = tokio::fs::metadata(&self.path)
            .await
            .map_err(|e| self.io_err(e))?;
        Ok(meta.len())
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let mut file = tokio::fs::File::open(&self.path)
            .await
            .map_err(|e| self.io_err(e))?;
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| self.io_err(e))?;
        let mut buf = vec![0u8; len];
        let mut filled = 0;
        loop {
            let n = file
                .read(&mut buf[filled..])
                .await
                .map_err(|e| self.io_err(e))?;
            if n == 0 {
                break;
            }
            filled += n;
            if filled == len {
                break;
            }
        }
        buf.truncate(filled);
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn reads_ranges_and_size_from_a_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&[10, 11, 12, 13, 14]).unwrap();
        let src = LocalFile::new(f.path());
        assert_eq!(src.size().await.unwrap(), 5);
        assert_eq!(src.read_at(1, 3).await.unwrap(), vec![11, 12, 13]);
    }

    #[tokio::test]
    async fn read_at_past_eof_returns_empty_not_an_error() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&[10, 11, 12, 13, 14]).unwrap();
        let src = LocalFile::new(f.path());
        assert_eq!(src.read_at(5, 3).await.unwrap(), Vec::<u8>::new());
    }
}
