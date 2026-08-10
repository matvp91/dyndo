use std::sync::Arc;

use super::super::super::packaging::wvtt::{WvttPackager, WvttSample};
use super::super::super::packaging::{MediaSegment, Sample};
use super::super::super::segmentation::{DurationPolicy, SegmentationPolicy, Segmenter};
use super::super::super::text::{timeline, vtt};
use ::opendal::raw::oio::{Read, ReadStream, StreamRead};
use ::opendal::raw::{
    Layer, OpCopier, OpCopy, OpCreateDir, OpList, OpPresign, OpRead, OpRename, OpStat, OpWrite,
    RpCreateDir, RpPresign, RpRead, RpRename, RpStat, Service, ServiceInfo, Servicer, oio,
};
use ::opendal::{
    Buffer, BytesRange, Capability, EntryMode, Error, ErrorKind, Metadata, OperationContext, Result,
};

const TIMESCALE: u32 = 1_000;

#[derive(Debug, Clone)]
pub(super) struct VttLayer {
    segmenter: Segmenter,
}

impl VttLayer {
    pub(super) fn new(boundaries: &[u32], text_length: u32) -> Self {
        Self {
            segmenter: Segmenter::new(SegmentationPolicy::new(
                boundaries,
                DurationPolicy::Exact(text_length),
            )),
        }
    }

    async fn package(
        &self,
        inner: &Servicer,
        ctx: &OperationContext,
        path: &str,
    ) -> Result<Buffer> {
        let (_, mut stream) = inner
            .read(ctx, path, OpRead::default())?
            .open(BytesRange::default())
            .await?;
        let document = String::from_utf8(stream.read_all().await?.to_vec()).map_err(unpackable)?;
        let subtitle = vtt::parse(&document).map_err(unpackable)?;
        let duration = subtitle.cues.iter().map(|cue| cue.end).max().unwrap_or(0);
        let segments = self
            .segmenter
            .exact(duration)
            .into_iter()
            .map(|range| {
                let samples = timeline::samples(&subtitle, range.clone())
                    .into_iter()
                    .map(|sample| {
                        let cues = sample.cues().iter().map(|cue| cue.text.clone()).collect();
                        Sample::new(sample.duration(), WvttSample::new(cues))
                    })
                    .collect();
                MediaSegment::new(u64::from(range.start), samples)
            })
            .collect::<Vec<_>>();
        let packaged = WvttPackager::new(TIMESCALE)
            .package(&segments)
            .map_err(unpackable)?;

        Ok(Buffer::from(packaged))
    }
}

impl Layer for VttLayer {
    fn apply_service(&self, inner: Servicer) -> Servicer {
        Arc::new(VttService {
            inner,
            layer: self.clone(),
        })
    }
}

fn unpackable(source: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::new(ErrorKind::Unexpected, "cannot package subtitle document").set_source(source)
}

fn is_subtitle(path: &str) -> bool {
    path.ends_with(".vtt")
}

#[derive(Debug)]
struct VttService {
    inner: Servicer,
    layer: VttLayer,
}

impl Service for VttService {
    type Reader = oio::Reader;
    type Writer = oio::Writer;
    type Lister = oio::Lister;
    type Deleter = oio::Deleter;
    type Copier = oio::Copier;

    async fn stat(&self, ctx: &OperationContext, path: &str, args: OpStat) -> Result<RpStat> {
        if !is_subtitle(path) {
            return self.inner.stat(ctx, path, args).await;
        }

        let packaged = self.layer.package(&self.inner, ctx, path).await?;
        Ok(RpStat::new(
            Metadata::new(EntryMode::FILE).with_content_length(packaged.len() as u64),
        ))
    }

    fn read(&self, ctx: &OperationContext, path: &str, args: OpRead) -> Result<Self::Reader> {
        if !is_subtitle(path) {
            return self.inner.read(ctx, path, args);
        }

        Ok(Box::new(oio::StreamReader::new(PackagedReader {
            inner: self.inner.clone(),
            ctx: ctx.clone(),
            path: path.to_string(),
            layer: self.layer.clone(),
        })))
    }

    fn info(&self) -> ServiceInfo {
        self.inner.info()
    }

    fn capability(&self) -> Capability {
        self.inner.capability()
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

struct PackagedReader {
    inner: Servicer,
    ctx: OperationContext,
    path: String,
    layer: VttLayer,
}

impl StreamRead for PackagedReader {
    async fn open(&self, range: BytesRange) -> Result<(RpRead, Box<dyn oio::ReadStreamDyn>)> {
        let packaged = self
            .layer
            .package(&self.inner, &self.ctx, &self.path)
            .await?;
        let length = packaged.len();
        let content = packaged.slice(range.to_content_range(length)?);
        let metadata = Metadata::new(EntryMode::FILE).with_content_length(length as u64);

        Ok((RpRead::new(metadata), Box::new(content)))
    }
}
