//! Another site's page cannot drive the machine: no cross-site writes, no rebound names, JSON routes take only JSON.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use support::Server;

const FORM: &str = "application/x-www-form-urlencoded";
const SEND: &str = "gcode=G91%0AG0+X10";

async fn call(server: &Server, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> (StatusCode, Value) {
    let request = headers.iter().fold(Request::builder().method(method).uri(path), |r, (k, v)| r.header(*k, *v));
    let response =
        tower::ServiceExt::oneshot(server.app.clone(), request.body(Body::from(body.to_string())).unwrap()).await;
    let response = response.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn sent_gcode(server: &Server) -> bool {
    server.settle().await;
    server.machine.snapshot().custom_gcode.is_some()
}

#[tokio::test]
async fn a_cross_site_form_post_is_refused_before_it_moves_the_machine() {
    let server = Server::new();
    let cases: &[&[(&str, &str)]] = &[
        &[("host", "maslow.local:5000"), ("origin", "http://evil.example")],
        &[("host", "maslow.local:5000"), ("origin", "http://maslow.local:5000.evil.example")],
        &[("host", "maslow.local:5000"), ("origin", "null")],
        &[("host", "maslow.local:5000"), ("sec-fetch-site", "cross-site"), ("origin", "http://maslow.local:5000")],
        &[("host", "maslow.local:5000"), ("sec-fetch-site", "same-site")],
    ];
    for headers in cases {
        let (status, body) =
            call(&server, "POST", "/sendGCode", &[&[("content-type", FORM)], *headers].concat(), SEND).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{headers:?}");
        assert!(body["error"].as_str().unwrap().contains("another site"));
    }
    assert!(!sent_gcode(&server).await);
}

#[tokio::test]
async fn a_cross_site_text_plain_post_to_a_json_route_is_refused() {
    let server = Server::new();
    let headers = [("host", "10.0.0.5:5000"), ("origin", "http://evil.example"), ("content-type", "text/plain")];
    for path in ["/api/system/pid-test", "/api/calibration/holey/accept", "/api/system/updates/install"] {
        assert_eq!(call(&server, "POST", path, &headers, "{}").await.0, StatusCode::FORBIDDEN, "{path}");
    }
}

#[tokio::test]
async fn the_ui_on_its_own_origin_and_scripts_without_browser_headers_are_allowed() {
    let server = Server::new();
    let same_origin = [
        ("host", "maslow.local:5000"),
        ("origin", "http://Maslow.local:5000"),
        ("sec-fetch-site", "same-origin"),
        ("content-type", FORM),
    ];
    assert_eq!(call(&server, "POST", "/sendGCode", &same_origin, SEND).await.0, StatusCode::OK);
    assert!(sent_gcode(&server).await);
    let curl = [("host", "anything.example.com"), ("content-type", FORM)];
    assert_eq!(call(&server, "POST", "/sendGCode", &curl, SEND).await.0, StatusCode::OK);
    let through_vite = [("host", "localhost:5173"), ("origin", "http://localhost:5173"), ("content-type", FORM)];
    assert_eq!(call(&server, "POST", "/sendGCode", &through_vite, SEND).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_browser_reaching_a_name_public_dns_could_rebind_is_refused() {
    let server = Server::new();
    let rebound = [("host", "rebind.evil.example:5000"), ("origin", "http://rebind.evil.example:5000")];
    let (status, body) =
        call(&server, "POST", "/sendGCode", &[&rebound[..], &[("content-type", FORM)]].concat(), SEND).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].as_str().unwrap().contains(".local"));
    let fetch_only = [("host", "rebind.evil.example"), ("sec-fetch-site", "same-origin")];
    assert_eq!(call(&server, "POST", "/api/calibration/holey/accept", &fetch_only, "").await.0, StatusCode::FORBIDDEN);
    let local = [
        "maslow.local",
        "maslow.local.",
        "maslow.lan:5000",
        "maslow",
        "localhost:5000",
        "192.168.86.155:5000",
        "[::1]:5000",
        "pi.home.arpa",
    ];
    for host in local {
        let headers = [("host", host), ("origin", &format!("http://{host}")), ("content-type", FORM)];
        assert_eq!(call(&server, "POST", "/sendGCode", &headers, SEND).await.0, StatusCode::OK, "{host}");
    }
}

#[tokio::test]
async fn reads_are_not_guarded() {
    let server = Server::new();
    let headers =
        [("host", "rebind.evil.example"), ("origin", "http://evil.example"), ("sec-fetch-site", "cross-site")];
    assert_eq!(call(&server, "GET", "/api/state", &headers, "").await.0, StatusCode::OK);
    assert_eq!(call(&server, "HEAD", "/", &headers, "").await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_json_route_refuses_a_body_of_another_type() {
    let server = Server::new();
    for content_type in ["text/plain", FORM, "multipart/form-data; boundary=x"] {
        let (status, body) =
            call(&server, "POST", "/api/system/pid-test", &[("content-type", content_type)], "{}").await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{content_type}");
        assert!(body["error"].as_str().unwrap().contains("application/json"));
    }
    assert_eq!(call(&server, "PUT", "/api/display/theme", &[], r#"{"theme": "dark"}"#).await.0, 415);
    let json = [("content-type", "Application/JSON; charset=utf-8")];
    assert_eq!(call(&server, "PUT", "/api/display/theme", &json, r#"{"theme": "dark"}"#).await.0, StatusCode::OK);
    let bodiless = call(&server, "POST", "/api/calibration/holey/accept", &[], "").await;
    assert_eq!(bodiless.0, StatusCode::CONFLICT, "reaches the route: nothing calculated to accept");
}
