use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_dash::options::DashOptions;
use dyndo_image::sprite::Sprite;
use opendal::Operator;

use super::context::RequestContext;
use super::segment::read_track;
use crate::error::ServerError;

/// Serves the sprite whose first thumbnail shows `time`, cut from the track the
/// manifest named in its URL.
pub(super) async fn image(
    op: &Operator,
    context: &RequestContext<DashOptions>,
    track_id: &str,
    time: u64,
) -> Result<Response, ServerError> {
    let (track, _) = read_track(op, context, track_id).await?;
    let sprite = Sprite {
        tile_size: context.manifest_options.thumbnail_tile_size,
        step: context.manifest_options.thumbnail_step,
        time,
    };
    let bytes = sprite.generate(op, &track).await?;

    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
