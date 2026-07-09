mod config;
mod error;
mod routes;

use std::sync::Arc;

use config::Config;
use routes::build_router;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = Config::load()?;
    // Canonicalise the base once; fall back to the raw path if it doesn't exist yet.
    config.assets_base_path = config
        .assets_base_path
        .canonicalize()
        .unwrap_or_else(|_| config.assets_base_path.clone());
    let config = Arc::new(config);

    let app = build_router(config.clone());
    let addr = ("0.0.0.0", config.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("dyndo-server listening on http://0.0.0.0:{}", config.port);
    axum::serve(listener, app).await?;
    Ok(())
}
