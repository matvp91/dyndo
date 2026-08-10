use axum::{
    http::header::CONTENT_TYPE,
    response::{IntoResponse, Response},
};
use dyndo_dash::{DashError, generate_thumbnail, options::DashOptions};
use opendal::Operator;

use super::context::RequestContext;
use super::segment::read_track;
use crate::error::ServerError;

/// Serves the thumbnail sprite named by the DASH `$Number$` substitution.
pub(super) async fn image(
    op: &Operator,
    context: &RequestContext<DashOptions>,
    track_id: &str,
    number: u64,
) -> Result<Response, ServerError> {
    let (track, _) = read_track(op, context, track_id).await?;
    let bytes = generate_thumbnail(op, &track, &context.manifest_options, number)
        .await
        .map_err(DashError::from)?;

    Ok(([(CONTENT_TYPE, "image/jpeg")], bytes).into_response())
}
