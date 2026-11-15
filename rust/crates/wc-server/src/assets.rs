//! Files compiled into the binary: the built React UI and the images its screens show.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../../frontend/dist"]
struct Dist;

#[derive(RustEmbed)]
#[folder = "../../../static/images"]
#[prefix = "images/"]
struct Images;

/// The embedded `frontend/dist` file names.
pub fn dist_files() -> Vec<String> {
    Dist::iter().map(|name| name.into_owned()).collect()
}

fn file(data: rust_embed::EmbeddedFile) -> Response {
    let mime = data.metadata.mimetype().to_string();
    ([(header::CONTENT_TYPE, mime)], data.data).into_response()
}

pub(crate) fn not_found_html() -> Response {
    let page = "<!doctype html>\n<title>404 Not Found</title>\n<h1>Not Found</h1>\n";
    (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
}

/// `next_ui.py`: a built file, a 404 for a missing path whose last segment has a dot, else `index.html`.
pub(crate) fn next(path: &str) -> Response {
    if let Some(data) = Dist::get(path).filter(|_| !path.is_empty()) {
        return file(data);
    }
    if path.rsplit('/').next().is_some_and(|last| last.contains('.')) {
        return (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "Not found")
            .into_response();
    }
    let mut response = file(Dist::get("index.html").expect("build.rs checks index.html"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// `/static/{filename}`; only `static/images` is embedded.
pub(crate) fn image(filename: &str) -> Response {
    Images::get(filename).map(file).unwrap_or_else(not_found_html)
}
