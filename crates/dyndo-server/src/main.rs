mod error;
mod routes;

use opendal::services::Fs;
use opendal::Operator;
use routes::build_router;

const PORT: u16 = 8080;
/// Filesystem root every asset key is resolved against.
const ASSETS_ROOT: &str = "./assets";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let op = Operator::new(Fs::default().root(ASSETS_ROOT))?;
    let app = build_router(op);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", PORT)).await?;
    println!("dyndo-server listening on http://0.0.0.0:{PORT}");
    axum::serve(listener, app).await?;
    Ok(())
}
