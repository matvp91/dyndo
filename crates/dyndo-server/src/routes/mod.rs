//! Route table. All DASH routes live under `/{asset}/dash/`.

mod dash;

use std::sync::Arc;

use axum::{routing::get, Router};
use tower_http::cors::{Any, CorsLayer};

use crate::config::Config;

pub(crate) fn build_router(config: Arc<Config>) -> Router {
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any);
    Router::new()
        .route("/{asset}/dash/index.mpd", get(dash::manifest))
        .route("/{asset}/dash/{repr}/{seg}", get(dash::segment))
        .with_state(config)
        .layer(cors)
}
