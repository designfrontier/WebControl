//! Refuses the requests another site's page could make to drive the machine: state-changing ones and WebSocket
//! upgrades that come from another origin, or through a DNS name rebound to this machine.

use std::net::IpAddr;

use axum::Json;
use axum::body::HttpBody;
use axum::extract::Request;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Names public DNS does not answer for, so a page cannot rebind one of them to this machine.
const LOCAL_SUFFIXES: &[&str] = &[".local", ".lan", ".home", ".home.arpa", ".internal", ".localdomain", ".localhost"];

const CROSS_SITE: &str = "Refused a request from another site: open WebControl itself to drive the machine.";
const NOT_LOCAL: &str = "Refused a browser request to an unknown name: open WebControl by its IP address, \
                         <hostname>.local or localhost.";
const NOT_JSON: &str = "Send the body as Content-Type: application/json.";

fn text(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).map(|value| value.to_str().unwrap_or_default())
}

fn same_origin(origin: &str, host: &str) -> bool {
    let authority = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://"));
    authority.is_some_and(|authority| authority.eq_ignore_ascii_case(host))
}

/// `Sec-Fetch-Site` is the browser's own verdict, so it decides when sent; requests with neither header (curl,
/// scripts, `webcontrol-update`) are not a browser's.
fn cross_site(headers: &HeaderMap) -> bool {
    let host = text(headers, header::HOST).unwrap_or_default();
    match (text(headers, header::HeaderName::from_static("sec-fetch-site")), text(headers, header::ORIGIN)) {
        (Some(site), _) => !matches!(site, "same-origin" | "none"),
        (None, Some(origin)) => !same_origin(origin, host),
        (None, None) => false,
    }
}

fn from_browser(headers: &HeaderMap) -> bool {
    headers.contains_key(header::ORIGIN) || headers.contains_key("sec-fetch-site")
}

fn local_name(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => host.split(':').next().unwrap_or_default(),
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name.parse::<IpAddr>().is_ok()
        || (!name.is_empty() && !name.contains('.'))
        || LOCAL_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

fn is_json(headers: &HeaderMap) -> bool {
    let content_type = text(headers, header::CONTENT_TYPE).unwrap_or_default();
    content_type.split(';').next().unwrap_or_default().trim().eq_ignore_ascii_case("application/json")
}

fn refuse(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({"error": error}))).into_response()
}

pub(crate) async fn guard(request: Request, next: Next) -> Response {
    let headers = request.headers();
    let writes = !matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let guarded = writes || headers.contains_key(header::UPGRADE);
    let host = text(headers, header::HOST).unwrap_or_default();
    if guarded && cross_site(headers) {
        tracing::warn!(origin = ?headers.get(header::ORIGIN), uri = %request.uri(), "refused a cross-site request");
        return refuse(StatusCode::FORBIDDEN, CROSS_SITE);
    }
    // Reads too: a rebound page could otherwise read back the settings and the backup.
    if from_browser(headers) && !local_name(host) {
        tracing::warn!(host, uri = %request.uri(), "refused a browser request to a non-local name");
        return refuse(StatusCode::FORBIDDEN, NOT_LOCAL);
    }
    if writes && request.uri().path().starts_with("/api/") && !request.body().is_end_stream() && !is_json(headers) {
        return refuse(StatusCode::UNSUPPORTED_MEDIA_TYPE, NOT_JSON);
    }
    next.run(request).await
}
