mod config;
mod error;
mod routes;

use routes::build_router;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // FFmpeg logs directly to stderr. Configure its process-wide level before
    // the server handles requests that may create codec contexts.
    // SAFETY: FFmpeg documents this setter as accepting logging constants; it
    // is called once during startup, before any FFmpeg work begins.
    unsafe {
        rsmpeg::ffi::av_log_set_level(rsmpeg::ffi::AV_LOG_QUIET);
    }
    // Install opendal's default HTTP transport (reqwest). With
    // `default-features = false` there is no ctor-based auto-install, and
    // network-backed services (s3) error at first read without it.
    opendal::install_default();
    let cfg = config::load()?;
    let op = cfg.build_operator()?;
    let (host, port) = cfg.bind();
    let app = build_router(op);
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    println!("dyndo-server listening on http://{host}:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}
