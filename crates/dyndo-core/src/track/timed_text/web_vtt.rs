use bytes::Bytes;
use opendal::Operator;
use relative_path::{RelativePath, RelativePathBuf};

use super::{ResolvedTimedTextTrack, TimedTextError, TimedTextFormat};
use crate::packaging::PackageError;
use crate::text::Subtitle;
use crate::track::cmaf::{CmafKind, ResolvedCmafTrack};
use crate::track::metadata::TextMetadata;

#[derive(Debug, thiserror::Error)]
pub enum WebVttPackageError {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Cmaf(#[from] crate::track::cmaf::CmafError),
}

impl ResolvedTimedTextTrack {
    pub(crate) async fn from_web_vtt_source(
        op: &Operator,
        path: &RelativePath,
        id: String,
        metadata: TextMetadata,
    ) -> Result<Self, TimedTextError> {
        let document = String::from_utf8(op.read(path.as_str()).await?.to_bytes().to_vec())?;
        Self::from_web_vtt_text(id, path.to_owned(), metadata, &document)
    }

    /// Creates a resolved timed-text track from a WebVTT document.
    pub fn from_web_vtt_text(
        id: String,
        source_path: RelativePathBuf,
        metadata: TextMetadata,
        document: &str,
    ) -> Result<Self, TimedTextError> {
        Ok(Self::new(
            id,
            source_path,
            TimedTextFormat::WebVtt,
            metadata,
            Subtitle::from_vtt_text(document)?,
        ))
    }

    fn package_bytes(&self, text_length: u32, boundaries: &[u32]) -> Result<Bytes, PackageError> {
        self.subtitle
            .to_wvtt(text_length, boundaries)
            .map(Bytes::from)
    }

    /// Packages this source as temporary, in-memory CMAF media.
    pub async fn package_wvtt(
        &self,
        text_length: u32,
        boundaries: &[u32],
    ) -> Result<ResolvedCmafTrack, WebVttPackageError> {
        let bytes = self.package_bytes(text_length, boundaries)?;
        ResolvedCmafTrack::from_cmaf_bytes(
            bytes,
            self.id().to_string(),
            CmafKind::Text(self.text_metadata().clone()),
        )
        .await
        .map_err(Into::into)
    }

    /// Returns the raw WebVTT document addressed by a served segment start time.
    pub async fn served_web_vtt_segment(
        &self,
        time: u64,
        min_length: u32,
        text_length: u32,
        boundaries: &[u32],
    ) -> Result<Option<String>, WebVttPackageError> {
        let cmaf = self.package_wvtt(text_length, boundaries).await?;
        let Some(segment) = cmaf.served_segment(time, min_length, boundaries) else {
            return Ok(None);
        };

        Ok(self.web_vtt_segment(segment.start_time(), segment.end_time()))
    }

    fn web_vtt_segment(&self, start: u64, end: u64) -> Option<String> {
        let start = u32::try_from(start).ok()?;
        let end = u32::try_from(end).ok()?;
        self.subtitle
            .slice(start, end)
            .map(|subtitle| subtitle.to_vtt_text())
    }
}
