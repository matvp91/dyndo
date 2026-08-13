use bytes::Bytes;
use opendal::Operator;

use super::frame_decoder::FrameDecoder;
use super::sprite_encoder::{SpriteEncoder, SpriteLayout};
use crate::track::cmaf::{CmafMetadata, ResolvedCmafTrack, Segment};

/// An error encountered while generating a thumbnail sprite.
#[derive(Debug, thiserror::Error)]
pub enum SpriteGeneratorError {
    #[error("invalid sprite request: {0}")]
    Invalid(String),
    #[error("could not read sprite media: {0}")]
    Read(String),
    #[error("could not decode sprite: {0}")]
    Decode(String),
    #[error("could not encode sprite: {0}")]
    Encode(String),
    #[error("sprite renderer failed: {0}")]
    Render(String),
}

/// Generates JPEG sprite images from the first frame of regularly spaced CMAF segments.
pub struct Sprite<'a> {
    op: &'a Operator,
    track: &'a ResolvedCmafTrack,
    tile_width: u32,
    tile_size: u32,
}

impl<'a> Sprite<'a> {
    pub fn new(
        op: &'a Operator,
        track: &'a ResolvedCmafTrack,
        tile_width: u32,
        tile_size: u32,
    ) -> Self {
        Self {
            op,
            track,
            tile_width,
            tile_size,
        }
    }

    pub async fn jpeg(&self, number: u32) -> Result<Bytes, SpriteGeneratorError> {
        let plan = SpritePlan::new(self.track, number, self.tile_width, self.tile_size)?;
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let renderer =
            tokio::task::spawn_blocking(move || render(receiver, plan.targets, plan.layout));
        let (producer, renderer) = tokio::join!(
            stream_fragments(self.op, self.track, &plan.segments, sender),
            renderer,
        );
        producer?;
        renderer.map_err(|error| SpriteGeneratorError::Render(error.to_string()))?
    }
}

struct SpritePlan<'a> {
    layout: SpriteLayout,
    segments: Vec<&'a Segment>,
    targets: Vec<u64>,
}

impl<'a> SpritePlan<'a> {
    fn new(
        track: &'a ResolvedCmafTrack,
        number: u32,
        tile_width: u32,
        tile_size: u32,
    ) -> Result<Self, SpriteGeneratorError> {
        let CmafMetadata::Video(video) = track.metadata() else {
            return Err(SpriteGeneratorError::Invalid(
                "source is not video".to_owned(),
            ));
        };
        if tile_width == 0 || tile_size == 0 || video.width == 0 || video.height == 0 {
            return Err(SpriteGeneratorError::Invalid(
                "dimensions must be greater than zero".to_owned(),
            ));
        }
        let invalid = || SpriteGeneratorError::Invalid("dimensions are too large".to_owned());
        let tile_height =
            u32::try_from(u64::from(tile_width) * u64::from(video.height) / u64::from(video.width))
                .map_err(|_| invalid())?;
        let width = tile_width.checked_mul(tile_size).ok_or_else(invalid)?;
        let height = tile_height.checked_mul(tile_size).ok_or_else(invalid)?;
        let tile_count = tile_size.checked_mul(tile_size).ok_or_else(invalid)?;
        let first = number
            .checked_mul(tile_count)
            .and_then(|first| usize::try_from(first).ok())
            .ok_or_else(invalid)?;
        let segments: Vec<_> = track
            .cadence_aligned_segments()
            .skip(first)
            .take(tile_count as usize)
            .collect();
        let targets = segments
            .iter()
            .map(|segment| segment.start_time())
            .collect();

        Ok(Self {
            layout: SpriteLayout {
                tile_width,
                tile_height,
                tile_size,
                width,
                height,
            },
            segments,
            targets,
        })
    }
}

async fn stream_fragments(
    op: &Operator,
    track: &ResolvedCmafTrack,
    segments: &[&Segment],
    sender: tokio::sync::mpsc::Sender<Bytes>,
) -> Result<(), SpriteGeneratorError> {
    let ranges = std::iter::once(track.init_segment().byte_range())
        .chain(segments.iter().map(|segment| segment.byte_range()));
    for range in ranges {
        let permit = sender
            .reserve()
            .await
            .map_err(|_| SpriteGeneratorError::Decode("decoder stopped reading".to_owned()))?;
        let bytes = track
            .read_range(op, range)
            .await
            .map_err(|error| SpriteGeneratorError::Read(error.to_string()))?;
        permit.send(bytes);
    }
    Ok(())
}

fn render(
    receiver: tokio::sync::mpsc::Receiver<Bytes>,
    targets: Vec<u64>,
    layout: SpriteLayout,
) -> Result<Bytes, SpriteGeneratorError> {
    let mut decoder = FrameDecoder::new(receiver)
        .map_err(|error| SpriteGeneratorError::Decode(error.to_string()))?;
    let mut encoder = SpriteEncoder::new(layout)
        .map_err(|error| SpriteGeneratorError::Encode(error.to_string()))?;
    for (index, target) in targets.into_iter().enumerate() {
        let Some(frame) = decoder
            .frame_at(target)
            .map_err(|error| SpriteGeneratorError::Decode(error.to_string()))?
        else {
            break;
        };
        encoder
            .add(&frame, index)
            .map_err(|error| SpriteGeneratorError::Encode(error.to_string()))?;
    }
    encoder
        .jpeg()
        .map_err(|error| SpriteGeneratorError::Encode(error.to_string()))
}
