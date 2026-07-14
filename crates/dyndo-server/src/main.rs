//! dyndo-server: the HTTP entry point. Loads config, builds the storage
//! operator, and serves the router over TCP.

mod config;
mod error;
mod routes;

use routes::build_router;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let op = cfg.build_operator()?;
    let (host, port) = cfg.bind();
    let app = build_router(op);
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    println!("dyndo-server listening on http://{host}:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}
