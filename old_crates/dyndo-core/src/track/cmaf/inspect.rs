use bytes::Bytes;
use language_tags::LanguageTag;
use mp4_atom::{Codec as Mp4Codec, FourCC};
use opendal::Operator;
use relative_path::RelativePath;

use super::boxes::{self, Boxes};
use super::segments::{build_init_segment, build_segments};
use super::{CmafError, CmafMetadata, CmafTrack, ResolvedCmafTrack};
use crate::codec::{
    AacCodec, Ac3Codec, Av1Codec, AvcCodec, CodecConfig, Eac3Codec, HevcCodec, WvttCodec,
};
use crate::track::metadata::{AudioMetadata, TextMetadata, VideoMetadata, undetermined_language};

impl CmafTrack {
    /// Resolves this configured CMAF source.
    pub async fn resolve(
        &self,
        op: &Operator,
        path: &RelativePath,
    ) -> Result<ResolvedCmafTrack, CmafError> {
        ResolvedCmafTrack::from_stored_cmaf(op, path, self.id.clone(), Some(self.metadata.clone()))
            .await
    }
}

impl ResolvedCmafTrack {
    pub(crate) async fn discover(
        op: &Operator,
        path: &RelativePath,
        id: String,
    ) -> Result<Self, CmafError> {
        Self::from_stored_cmaf(op, path, id, None).await
    }

    async fn from_stored_cmaf(
        op: &Operator,
        path: &RelativePath,
        id: String,
        configured_metadata: Option<CmafMetadata>,
    ) -> Result<Self, CmafError> {
        let boxes = boxes::scan(op, path.as_str()).await?;
        let (metadata, init_segment, segments) = inspect(&boxes, configured_metadata)?;

        Ok(Self::new(
            id,
            path.to_owned(),
            metadata,
            init_segment,
            segments,
        ))
    }

    /// Creates resolved CMAF media backed by serialized bytes.
    pub(crate) async fn from_cmaf_bytes(
        bytes: Bytes,
        id: String,
        configured_metadata: CmafMetadata,
    ) -> Result<Self, CmafError> {
        let boxes = boxes::scan_bytes(bytes.clone()).await?;
        let (metadata, init_segment, segments) = inspect(&boxes, Some(configured_metadata))?;

        Ok(Self::from_memory(
            id,
            bytes,
            metadata,
            init_segment,
            segments,
        ))
    }
}

fn inspect(
    boxes: &Boxes,
    configured_metadata: Option<CmafMetadata>,
) -> Result<
    (
        CmafMetadata,
        std::sync::Arc<super::InitSegment>,
        Vec<super::Segment>,
    ),
    CmafError,
> {
    let init_segment = build_init_segment(boxes, build_codec(boxes)?);
    let discovered_metadata = build_metadata(boxes)?;
    let segments = build_segments(boxes, &init_segment)?;

    Ok((
        configured_metadata.unwrap_or(discovered_metadata),
        init_segment,
        segments,
    ))
}

fn build_codec(boxes: &Boxes) -> Result<CodecConfig, CmafError> {
    let codec = &boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    match codec {
        Mp4Codec::Avc1(entry) => Ok(CodecConfig::Avc(AvcCodec::new(entry))),
        Mp4Codec::Av01(entry) => Ok(CodecConfig::Av1(Av1Codec::new(entry))),
        Mp4Codec::Hvc1(entry) => Ok(CodecConfig::Hevc(HevcCodec::new(entry))),
        Mp4Codec::Hev1(entry) => Ok(CodecConfig::Hevc(HevcCodec::new(entry))),
        Mp4Codec::Mp4a(entry) => Ok(CodecConfig::Aac(AacCodec::new(entry))),
        Mp4Codec::Ac3(_) => Ok(CodecConfig::Ac3(Ac3Codec)),
        Mp4Codec::Eac3(_) => Ok(CodecConfig::Eac3(Eac3Codec)),
        Mp4Codec::Wvtt(_) => Ok(CodecConfig::Wvtt(WvttCodec)),
        codec => Err(CmafError::UnsupportedCodec(codec_name(codec))),
    }
}

fn build_metadata(boxes: &Boxes) -> Result<CmafMetadata, CmafError> {
    let handler = boxes.moov.trak[0].mdia.hdlr.handler;

    if handler == FourCC::new(b"vide") {
        build_video_metadata(boxes).map(CmafMetadata::Video)
    } else if handler == FourCC::new(b"soun") {
        build_audio_metadata(boxes).map(CmafMetadata::Audio)
    } else if handler == FourCC::new(b"text") {
        Ok(CmafMetadata::Text(build_text_metadata(boxes)))
    } else {
        Err(CmafError::UnsupportedTrackHandler)
    }
}

fn build_video_metadata(boxes: &Boxes) -> Result<VideoMetadata, CmafError> {
    let sample_entry = &boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    let visual = match sample_entry {
        Mp4Codec::Avc1(entry) => &entry.visual,
        Mp4Codec::Av01(entry) => &entry.visual,
        Mp4Codec::Hvc1(entry) => &entry.visual,
        Mp4Codec::Hev1(entry) => &entry.visual,
        _ => return Err(CmafError::UnsupportedVideoSampleEntry),
    };

    Ok(VideoMetadata {
        width: u32::from(visual.width),
        height: u32::from(visual.height),
        frame_rate: frame_rate(boxes)?,
    })
}

fn build_audio_metadata(boxes: &Boxes) -> Result<AudioMetadata, CmafError> {
    let sample_entry = &boxes.moov.trak[0].mdia.minf.stbl.stsd.codecs[0];
    let audio = match sample_entry {
        Mp4Codec::Mp4a(entry) => &entry.audio,
        Mp4Codec::Ac3(entry) => &entry.audio,
        Mp4Codec::Eac3(entry) => &entry.audio,
        _ => return Err(CmafError::UnsupportedAudioSampleEntry),
    };

    Ok(AudioMetadata {
        sample_rate: audio.sample_rate.integer() as u32,
        channels: audio.channel_count,
        language: language(boxes),
        role: None,
    })
}

fn build_text_metadata(boxes: &Boxes) -> TextMetadata {
    TextMetadata {
        language: language(boxes),
        role: None,
    }
}

fn frame_rate(boxes: &Boxes) -> Result<String, CmafError> {
    let track = &boxes.moov.trak[0];
    let sample_duration = boxes
        .moof
        .traf
        .first()
        .and_then(|fragment| fragment.trun.first())
        .and_then(|run| run.entries.first())
        .and_then(|sample| sample.duration)
        .filter(|duration| *duration != 0)
        .ok_or(CmafError::MissingFrameRate)?;
    let timescale = track.mdia.mdhd.timescale;
    if timescale == 0 {
        return Err(CmafError::MissingFrameRate);
    }
    let divisor = greatest_common_divisor(timescale, sample_duration);

    Ok(format!(
        "{}/{}",
        timescale / divisor,
        sample_duration / divisor
    ))
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn language(boxes: &Boxes) -> LanguageTag {
    let language = boxes.moov.trak[0].mdia.mdhd.language.as_str();
    language.parse().unwrap_or_else(|_| undetermined_language())
}

fn codec_name(codec: &Mp4Codec) -> String {
    format!("{codec:?}")
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or("unknown")
        .to_string()
}
