//! An opendal layer that serves subtitle documents as CMAF `wvtt` tracks.
//!
//! A read of a `.vtt` path fetches the document from the storage underneath,
//! packs it, and hands back the packed track's bytes. Nothing is written back,
//! and every other path passes straight through.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendal::raw::oio::{Read, ReadStream, StreamRead};
use opendal::raw::{
    Layer, OpCopier, OpCopy, OpCreateDir, OpList, OpPresign, OpRead, OpRename, OpStat, OpWrite,
    RpCreateDir, RpPresign, RpRead, RpRename, RpStat, Service, ServiceInfo, Servicer, oio,
};
use opendal::{
    Buffer, BytesRange, Capability, EntryMode, Error, ErrorKind, Metadata, OperationContext, Result,
};

use crate::{vtt, wvtt};

/// Serves `.vtt` documents as `wvtt` tracks, packed on read.
///
/// The tracks are fragmented at `boundaries_ms` and on the
/// `text_segment_length_ms` grid (see [`wvtt::pack`]), so the layer carries the
/// asset's segmentation policy and belongs to the operator that serves one
/// request, not to a process-wide operator.
#[derive(Debug)]
pub struct WvttLayer {
    transform: Arc<Transform>,
}

impl WvttLayer {
    pub fn new(boundaries_ms: &[u64], text_segment_length_ms: u64) -> Self {
        Self {
            transform: Arc::new(Transform {
                boundaries_ms: boundaries_ms.to_vec(),
                text_segment_length_ms,
                packed: Mutex::new(HashMap::new()),
            }),
        }
    }
}

impl Layer for WvttLayer {
    fn apply_service(&self, inner: Servicer) -> Servicer {
        Arc::new(WvttService {
            inner,
            transform: self.transform.clone(),
        })
    }
}

/// Packs subtitle documents, holding each result for as long as the layer lives:
/// reading a track walks its boxes and then asks for several byte ranges, and
/// each of those would otherwise pack the document again.
#[derive(Debug)]
struct Transform {
    boundaries_ms: Vec<u64>,
    text_segment_length_ms: u64,
    packed: Mutex<HashMap<String, Buffer>>,
}

impl Transform {
    async fn packed(&self, inner: &Servicer, ctx: &OperationContext, path: &str) -> Result<Buffer> {
        if let Some(packed) = self.packed.lock().unwrap().get(path).cloned() {
            return Ok(packed);
        }

        let document = self.document(inner, ctx, path).await?;
        let subtitle = vtt::parse(&document)
            .map_err(|error| invalid(path, "subtitle document does not parse", error))?;
        let packed = Buffer::from(
            wvtt::pack(&subtitle, &self.boundaries_ms, self.text_segment_length_ms)
                .map_err(|error| invalid(path, "subtitle document cannot be packed", error))?,
        );

        self.packed
            .lock()
            .unwrap()
            .insert(path.to_string(), packed.clone());
        Ok(packed)
    }

    async fn document(
        &self,
        inner: &Servicer,
        ctx: &OperationContext,
        path: &str,
    ) -> Result<String> {
        let reader = inner.read(ctx, path, OpRead::default())?;
        let (_, mut stream) = reader.open(BytesRange::default()).await?;
        let bytes = stream.read_all().await?;

        String::from_utf8(bytes.to_vec())
            .map_err(|error| invalid(path, "subtitle document is not valid UTF-8", error))
    }
}

fn invalid(
    path: &str,
    message: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> Error {
    Error::new(ErrorKind::Unexpected, message)
        .with_context("path", path.to_string())
        .set_source(source)
}

fn is_subtitle(path: &str) -> bool {
    path.ends_with(".vtt")
}

#[derive(Debug)]
struct WvttService {
    inner: Servicer,
    transform: Arc<Transform>,
}

impl Service for WvttService {
    type Reader = oio::Reader;
    type Writer = oio::Writer;
    type Lister = oio::Lister;
    type Deleter = oio::Deleter;
    type Copier = oio::Copier;

    fn info(&self) -> ServiceInfo {
        self.inner.info()
    }

    fn capability(&self) -> Capability {
        self.inner.capability()
    }

    async fn stat(&self, ctx: &OperationContext, path: &str, args: OpStat) -> Result<RpStat> {
        if !is_subtitle(path) {
            return self.inner.stat(ctx, path, args).await;
        }

        // The packed track's length, not the document's.
        let packed = self.transform.packed(&self.inner, ctx, path).await?;
        Ok(RpStat::new(
            Metadata::new(EntryMode::FILE).with_content_length(packed.len() as u64),
        ))
    }

    fn read(&self, ctx: &OperationContext, path: &str, args: OpRead) -> Result<Self::Reader> {
        if !is_subtitle(path) {
            return self.inner.read(ctx, path, args);
        }

        // Packing awaits and this does not, so the reader packs when it opens.
        Ok(Box::new(oio::StreamReader::new(PackedReader {
            inner: self.inner.clone(),
            ctx: ctx.clone(),
            path: path.to_string(),
            transform: self.transform.clone(),
        })))
    }

    async fn create_dir(
        &self,
        ctx: &OperationContext,
        path: &str,
        args: OpCreateDir,
    ) -> Result<RpCreateDir> {
        self.inner.create_dir(ctx, path, args).await
    }

    fn write(&self, ctx: &OperationContext, path: &str, args: OpWrite) -> Result<Self::Writer> {
        self.inner.write(ctx, path, args)
    }

    fn delete(&self, ctx: &OperationContext) -> Result<Self::Deleter> {
        self.inner.delete(ctx)
    }

    fn list(&self, ctx: &OperationContext, path: &str, args: OpList) -> Result<Self::Lister> {
        self.inner.list(ctx, path, args)
    }

    fn copy(
        &self,
        ctx: &OperationContext,
        from: &str,
        to: &str,
        args: OpCopy,
        opts: OpCopier,
    ) -> Result<Self::Copier> {
        self.inner.copy(ctx, from, to, args, opts)
    }

    async fn rename(
        &self,
        ctx: &OperationContext,
        from: &str,
        to: &str,
        args: OpRename,
    ) -> Result<RpRename> {
        self.inner.rename(ctx, from, to, args).await
    }

    async fn presign(
        &self,
        ctx: &OperationContext,
        path: &str,
        args: OpPresign,
    ) -> Result<RpPresign> {
        self.inner.presign(ctx, path, args).await
    }
}

/// Reads ranges of a packed track, packing it on first open.
struct PackedReader {
    inner: Servicer,
    ctx: OperationContext,
    path: String,
    transform: Arc<Transform>,
}

impl StreamRead for PackedReader {
    async fn open(&self, range: BytesRange) -> Result<(RpRead, Box<dyn oio::ReadStreamDyn>)> {
        let packed = self
            .transform
            .packed(&self.inner, &self.ctx, &self.path)
            .await?;
        let length = packed.len();
        let content = packed.slice(range.to_content_range(length)?);
        let metadata = Metadata::new(EntryMode::FILE).with_content_length(length as u64);

        Ok((RpRead::new(metadata), Box::new(content)))
    }
}

#[cfg(test)]
mod tests {
    use opendal::Operator;
    use opendal::services::Memory;

    use super::*;

    const DOCUMENT: &str = "WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n";

    #[tokio::test]
    async fn serves_a_subtitle_document_as_a_cmaf_track() {
        let op = operator("text.vtt", DOCUMENT).await;

        let track = op.read("text.vtt").await.unwrap().to_vec();

        assert_eq!(&track[4..8], b"ftyp");
        assert!(
            track.windows(4).any(|kind| kind == b"wvtt"),
            "packed track declares no wvtt sample entry"
        );
    }

    #[tokio::test]
    async fn stat_reports_the_packed_length() {
        let op = operator("text.vtt", DOCUMENT).await;

        let packed = op.read("text.vtt").await.unwrap().len();
        let stat = op.stat("text.vtt").await.unwrap();

        assert_eq!(usize::try_from(stat.content_length()).unwrap(), packed);
        assert_ne!(
            packed,
            DOCUMENT.len(),
            "stat reported the document's length"
        );
    }

    #[tokio::test]
    async fn serves_ranges_of_the_packed_track() {
        let op = operator("text.vtt", DOCUMENT).await;

        let whole = op.read("text.vtt").await.unwrap().to_vec();
        let range = op
            .read_with("text.vtt")
            .range(8..24)
            .await
            .unwrap()
            .to_vec();

        assert_eq!(range, whole[8..24]);
    }

    #[tokio::test]
    async fn leaves_other_paths_untouched() {
        let op = operator("track.mp4", "pretend this is CMAF").await;

        let bytes = op.read("track.mp4").await.unwrap().to_vec();

        assert_eq!(bytes, b"pretend this is CMAF");
    }

    #[tokio::test]
    async fn reports_a_document_that_does_not_parse() {
        let op = operator("text.vtt", "NOT A SUBTITLE").await;

        let error = op.read("text.vtt").await.unwrap_err();

        assert!(
            error
                .to_string()
                .contains("subtitle document does not parse"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn fragments_on_the_requested_grid() {
        let op = Operator::new(Memory::default()).unwrap();
        op.write("text.vtt", "WEBVTT\n\n00:00.000 --> 00:10.000\nA\n")
            .await
            .unwrap();

        let ungridded = op.clone().layer(WvttLayer::new(&[], 0));
        let gridded = op.clone().layer(WvttLayer::new(&[], 4_000));

        assert!(
            gridded.read("text.vtt").await.unwrap().len()
                > ungridded.read("text.vtt").await.unwrap().len(),
            "a finer grid should add fragments, and so bytes"
        );
    }

    async fn operator(path: &str, contents: &str) -> Operator {
        let op = Operator::new(Memory::default()).unwrap();
        op.write(path, contents.to_string()).await.unwrap();
        op.layer(WvttLayer::new(&[], 0))
    }
}
