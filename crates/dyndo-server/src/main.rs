mod config;
mod error;
mod path;
mod routes;
mod state;

use config::Config;
use routes::build_router;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    // Canonicalise the base once; fall back to the raw path if it doesn't exist yet.
    let base = config
        .assets_base_path
        .canonicalize()
        .unwrap_or_else(|_| config.assets_base_path.clone());

    let app = build_router(AppState::new(base));
    let addr = ("0.0.0.0", config.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("dyndo-server listening on http://0.0.0.0:{}", config.port);
    axum::serve(listener, app).await?;
    Ok(())
}
