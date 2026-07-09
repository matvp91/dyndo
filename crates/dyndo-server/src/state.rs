//! Shared, cheaply-cloneable server state.

use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) assets_base_path: Arc<PathBuf>,
}

impl AppState {
    pub(crate) fn new(assets_base_path: PathBuf) -> Self {
        Self {
            assets_base_path: Arc::new(assets_base_path),
        }
    }
}
