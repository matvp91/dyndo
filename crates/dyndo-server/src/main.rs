mod error;
mod routes;

use std::path::PathBuf;
use std::sync::Arc;

use routes::build_router;

const PORT: u16 = 8080;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Canonicalise the base once; fall back to the raw path if it doesn't exist yet.
    let assets_base = PathBuf::from("./assets");
    let assets_base = Arc::new(assets_base.canonicalize().unwrap_or(assets_base));

    let app = build_router(assets_base);
    let addr = ("0.0.0.0", PORT);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("dyndo-server listening on http://0.0.0.0:{PORT}");
    axum::serve(listener, app).await?;
    Ok(())
}
