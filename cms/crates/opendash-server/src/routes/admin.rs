use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../admin/dist"]
struct AdminFiles;

pub async fn index() -> Response {
    send_asset("index.html")
}

pub async fn asset(Path(path): Path<String>) -> Response {
    if path.split('/').any(|part| part == "." || part == "..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    send_asset(&format!("assets/{path}"))
}

fn send_asset(path: &str) -> Response {
    let Some(file) = AdminFiles::get(path) else {
        return (StatusCode::NOT_FOUND, "Admin asset not found").into_response();
    };

    let content_type = match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    };

    (
        [(header::CONTENT_TYPE, content_type)],
        file.data.into_owned(),
    )
        .into_response()
}
