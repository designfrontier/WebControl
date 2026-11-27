//! The classic form routes' bodies, as Flask's `request.form` and `request.files` read them, and their answers
//! (wart 8).

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use wc_protocol::files::{ClassicResult, ClassicStatus};

/// Uploads can be whole jobs; a 300k-line file is about 10 MB.
pub const UPLOAD_LIMIT: usize = 256 * 1024 * 1024;
const OUTSIDE: &str = "Path is outside the library";

#[derive(Debug, Clone, PartialEq)]
pub struct Upload {
    pub field: String,
    pub file_name: String,
    pub bytes: Bytes,
}

/// A form's text fields, first value wins, and its files.
#[derive(Debug, Default)]
pub struct Form {
    fields: Vec<(String, String)>,
    files: Vec<Upload>,
}

impl Form {
    /// The field, or Werkzeug's HTML 400 that a missing one gives.
    pub fn field(&self, name: &str) -> Result<&str, Response> {
        self.fields.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str()).ok_or_else(bad_request)
    }

    pub fn files(&self, field: &str) -> Vec<&Upload> {
        self.files.iter().filter(|upload| upload.field == field).collect()
    }

    pub fn file(&self, field: &str) -> Result<&Upload, Response> {
        self.files(field).into_iter().next().ok_or_else(bad_request)
    }

    async fn multipart(request: Request) -> Option<Form> {
        let mut multipart = Multipart::from_request(request, &()).await.ok()?;
        let mut form = Form::default();
        while let Some(field) = multipart.next_field().await.ok()? {
            let name = field.name().unwrap_or_default().to_string();
            match field.file_name().map(String::from) {
                Some(file_name) => form.files.push(Upload { field: name, file_name, bytes: field.bytes().await.ok()? }),
                None => form.fields.push((name, field.text().await.ok()?)),
            }
        }
        Some(form)
    }

    /// A multipart or URL-encoded body; anything unreadable is Werkzeug's 400.
    pub async fn read(request: Request) -> Result<Form, Response> {
        let multipart = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("multipart/form-data"));
        let form = match multipart {
            true => Form::multipart(request).await,
            false => {
                let body = axum::body::to_bytes(request.into_body(), UPLOAD_LIMIT).await.ok();
                body.and_then(|bytes| serde_urlencoded::from_bytes(&bytes).ok())
                    .map(|fields| Form { fields, files: Vec::new() })
            }
        };
        form.ok_or_else(bad_request)
    }
}

/// Werkzeug's page for a request it could not read, such as one missing a form field.
pub fn bad_request() -> Response {
    let page = "<!doctype html>\n<html lang=en>\n<title>400 Bad Request</title>\n<h1>Bad Request</h1>\n\
                <p>The browser (or proxy) sent a request that this server could not understand.</p>\n";
    (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], Body::from(page)).into_response()
}

/// `{"status": n}` with the HTTP status repeated.
pub fn status(code: u16) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(ClassicStatus::new(code))).into_response()
}

/// `{"status": n, "error": ...}`; the reason phrase stays the default (plan, wart 8).
pub fn refused(code: u16, error: &str) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(ClassicStatus::refused(code, error))).into_response()
}

/// A 200 whose load went ahead, with what the user should know about it.
pub fn warned(warning: &str) -> Response {
    Json(ClassicStatus::warned(200, warning)).into_response()
}

pub fn outside_library() -> Response {
    refused(400, OUTSIDE)
}

/// `"success"` or a 500 `"failed"`.
pub fn result(ok: bool) -> Response {
    match ok {
        true => Json(ClassicResult::Success).into_response(),
        false => (StatusCode::INTERNAL_SERVER_ERROR, Json(ClassicResult::Failed)).into_response(),
    }
}
