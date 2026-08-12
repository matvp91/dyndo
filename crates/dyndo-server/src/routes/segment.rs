use axum::{
    Router,
    extract::{Path, State},
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
    routing::get,
};
use dyndo_core::asset::Asset;
use dyndo_core::track::cmaf::ResolvedCmafTrack;
use dyndo_core::track::{CmafRepresentationError, ResolvedTrack};
use opendal::Operator;

use super::options::Options;
use super::{load_asset, resolve_track};
use crate::error::ServerError;

pub(super) trait SegmentRoute {
    /// Registers the per-track segment route.
    fn segment_route(self) -> Self;

    /// Serves the requested initialization, media, text, or image segment.
    async fn track_file(
        op: State<Operator>,
        path: Path<(String, String, String)>,
    ) -> Result<Response, ServerError>;
}

impl SegmentRoute for Router<Operator> {
    fn segment_route(self) -> Self {
        self.route("/out/{options}/{track_id}/{file}", get(Self::track_file))
    }

    async fn track_file(
        State(op): State<Operator>,
        Path((encoded_options, track_id, file)): Path<(String, String, String)>,
    ) -> Result<Response, ServerError> {
        let not_found = || ServerError::NotFound(file.clone());
        let (file_name, extension) = file.rsplit_once('.').ok_or_else(not_found)?;
        let options = Options::parse(&encoded_options)?;
        let asset = load_asset(&op, &options).await?;

        match (file_name, extension) {
            ("init", "mp4") => initialization(&op, &asset, options.text_length(), &track_id).await,
            (time, "m4s") => {
                media(
                    &op,
                    &asset,
                    options.min_length(),
                    options.text_length(),
                    &track_id,
                    segment_time(time, &file)?,
                )
                .await
            }
            (time, "vtt") => {
                text(
                    &op,
                    &asset,
                    options.min_length(),
                    options.text_length(),
                    &track_id,
                    segment_time(time, &file)?,
                )
                .await
            }
            (time, "jpg") => thumbnail(&op, &asset, &track_id, segment_time(time, &file)?).await,
            _ => Err(not_found()),
        }
    }
}

fn segment_time(name: &str, file: &str) -> Result<u64, ServerError> {
    name.parse()
        .map_err(|_| ServerError::NotFound(file.to_string()))
}

pub(super) async fn initialization(
    op: &Operator,
    asset: &Asset,
    text_length: u32,
    track_id: &str,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, track_id, None).await?;
    let cmaf = cmaf_representation(&track, text_length, &asset.boundaries).await?;
    let bytes = cmaf
        .read_range(op, cmaf.init_segment().byte_range())
        .await?;

    Ok(([(CONTENT_TYPE, cmaf.kind().mime_type())], bytes).into_response())
}

pub(super) async fn media(
    op: &Operator,
    asset: &Asset,
    min_length: u32,
    text_length: u32,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, track_id, None).await?;
    let cmaf = cmaf_representation(&track, text_length, &asset.boundaries).await?;
    let segment = cmaf
        .served_segment(time, min_length, &asset.boundaries)
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;
    let bytes = cmaf.read_range(op, segment.byte_range()).await?;

    Ok(([(CONTENT_TYPE, cmaf.kind().mime_type())], bytes).into_response())
}

pub(super) async fn text(
    op: &Operator,
    asset: &Asset,
    min_length: u32,
    text_length: u32,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, track_id, None).await?;
    let timed_text = track
        .timed_text()
        .ok_or_else(|| ServerError::NotFound(format!("track {track_id}")))?;
    let text = timed_text
        .served_web_vtt_segment(time, min_length, text_length, &asset.boundaries)
        .await
        .map_err(CmafRepresentationError::from)?
        .ok_or_else(|| ServerError::NotFound(format!("segment {time} for track {track_id}")))?;

    Ok(([(CONTENT_TYPE, "text/vtt")], text).into_response())
}

async fn cmaf_representation(
    track: &ResolvedTrack,
    text_length: u32,
    boundaries: &[u32],
) -> Result<ResolvedCmafTrack, ServerError> {
    track
        .cmaf_representation(text_length, boundaries)
        .await
        .map_err(Into::into)
}

/// Serves the thumbnail sprite named by a manifest segment number.
pub(super) async fn thumbnail(
    op: &Operator,
    asset: &Asset,
    thumbnail_id: &str,
    number: u64,
) -> Result<Response, ServerError> {
    let track = resolve_track(op, asset, thumbnail_id, None).await?;
    let thumbnail = track
        .thumbnail()
        .ok_or_else(|| ServerError::NotFound(format!("thumbnail {thumbnail_id}")))?;
    let Some(time) = thumbnail.time_for_number(number) else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };
    let Some(bytes) = thumbnail.jpeg(op, time).await? else {
        return Err(ServerError::NotFound("thumbnail".to_string()));
    };

    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
