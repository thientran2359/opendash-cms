mod app;
mod paths;
mod routes;
mod storage;
mod theme;

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let port: u16 = std::env::var("OPENDASH_PORT")
        .unwrap_or_else(|_| "18766".to_owned())
        .parse()?;
    let site_paths = paths::SitePaths::from_env()?;
    let database = storage::connect(&site_paths.db_path().to_string_lossy()).await?;

    // Local-only while auth, permission checks, and plugin sandbox are absent.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("OpenDash scaffold: http://127.0.0.1:{port}/");
    println!("React Admin preview: http://127.0.0.1:{port}/admin/");

    axum::serve(listener, app::router(database)).await?;
    Ok(())
}
