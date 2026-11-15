use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use wc_config::{Paths, Settings};
use wc_machine::{Machine, spawn};
use wc_server::{AppState, PingConfig, router};
use wc_testkit::contract::{Contract, ResponseBody};

struct Reply {
    status: StatusCode,
    content_type: String,
    cache_control: Option<String>,
    location: Option<String>,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    /// The reply as the contract harness sees it.
    fn checked(&self, method: &str, path: &str) -> &Self {
        let body = if self.content_type.starts_with("application/json") {
            ResponseBody::Json(self.json())
        } else {
            ResponseBody::Other(self.content_type.clone())
        };
        Contract::shared().validate_response(method, path, self.status.as_u16(), &body).unwrap();
        self
    }
}

fn settings(file: Option<Value>) -> (tempfile::TempDir, Settings) {
    let home = tempfile::tempdir().unwrap();
    if let Some(file) = file {
        std::fs::create_dir_all(home.path().join(".WebControl")).unwrap();
        std::fs::write(home.path().join(".WebControl/webcontrol.json"), file.to_string()).unwrap();
    }
    let settings = Settings::load(&Paths::new(home.path()));
    (home, settings)
}

async fn request_with(settings: Settings, method: &str, path: &str) -> Reply {
    let (machine, _task) = spawn(Machine::new("10.0.0.5:5000"), None);
    let app = router(AppState::new(machine, settings, PingConfig::default()));
    let request = Request::builder().method(method).uri(path).body(Body::empty()).unwrap();
    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    let header = |name: header::HeaderName| response.headers().get(name).map(|v| v.to_str().unwrap().to_string());
    let (content_type, cache_control, location) =
        (header(header::CONTENT_TYPE).unwrap_or_default(), header(header::CACHE_CONTROL), header(header::LOCATION));
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes().to_vec();
    Reply { status, content_type, cache_control, location, body }
}

async fn get(path: &str) -> Reply {
    request_with(settings(None).1, "GET", path).await
}

fn dist_asset() -> String {
    wc_server::assets::dist_files().into_iter().find(|f| f.ends_with(".js")).expect("a built .js file")
}

#[tokio::test]
async fn the_root_redirects_to_the_react_ui() {
    let reply = get("/").await;
    assert_eq!(reply.status, StatusCode::FOUND);
    assert_eq!(reply.location.as_deref(), Some("/next/"));
    let bare = get("/next").await;
    assert_eq!(bare.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(bare.location.as_deref(), Some("/next/"));
}

#[tokio::test]
async fn the_ui_is_served_from_the_embedded_dist() {
    let index = get("/next/").await;
    index.checked("GET", "/next/");
    assert_eq!(index.status, StatusCode::OK);
    assert!(index.content_type.starts_with("text/html"));
    assert!(index.text().contains("id=\"root\""));
    assert_eq!(index.cache_control.as_deref(), Some("no-cache"));
}

#[tokio::test]
async fn built_files_are_served_typed_by_extension() {
    let path = format!("/next/{}", dist_asset());
    let reply = get(&path).await;
    reply.checked("GET", &path);
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.content_type.contains("javascript"), "{}", reply.content_type);
    assert_eq!(reply.cache_control, None);
}

#[tokio::test]
async fn client_routes_fall_back_to_index_html() {
    let reply = get("/next/calibration/holey").await;
    reply.checked("GET", "/next/calibration/holey");
    assert!(reply.text().contains("id=\"root\""));
    assert_eq!(reply.cache_control.as_deref(), Some("no-cache"));
}

#[tokio::test]
async fn a_missing_file_with_an_extension_is_a_plain_404() {
    let reply = get("/next/assets/nope.js").await;
    reply.checked("GET", "/next/assets/nope.js");
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.text(), "Not found");
}

#[tokio::test]
async fn static_images_are_served() {
    let reply = get("/static/images/GPIO/RPI_pin_layout.svg").await;
    reply.checked("GET", "/static/images/GPIO/RPI_pin_layout.svg");
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.content_type.starts_with("image/svg+xml"));
}

#[tokio::test]
async fn static_files_outside_images_are_not() {
    for path in ["/static/images/nope.png", "/static/scripts/baseSocket.js", "/static/images/../../main.py"] {
        let reply = get(path).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert!(reply.content_type.starts_with("text/html"), "{path}");
    }
}

#[tokio::test]
async fn api_state_reports_the_settings_and_the_rust_backend() {
    let file = json!({
        "Computed Settings": [{"key": "units", "value": "MM"}, {"key": "distToMove", "value": "2.5"},
            {"key": "unitsZ", "value": "MM"}, {"key": "distToMoveZ", "value": 1}],
        "Advanced Settings": [{"key": "homeX", "value": "12.5"}, {"key": "homeY", "value": "-3"}],
        "Maslow Settings": [{"key": "macro1_title", "value": "Spindle on"}, {"key": "macro2_title", "value": ""}]
    });
    let reply = request_with(settings(Some(file)).1, "GET", "/api/state").await;
    reply.checked("GET", "/api/state");
    assert_eq!(
        reply.json(),
        json!({"units": "MM", "distToMove": 2.5, "unitsZ": "MM", "distToMoveZ": 1.0,
            "homePosition": {"x": 12.5, "y": -3.0}, "pauseButtonSetting": "Pause", "jobInProgress": false,
            "manualZAxisAdjust": false, "hostAddress": "10.0.0.5:5000", "controllerFirmwareVersion": 0.0,
            "controllerBoard": null, "firmwareBuild": null,
            "macros": [{"command": "macro1", "title": "Spindle on"}, {"command": "macro2", "title": ""}],
            "backend": "rust"})
    );
}

#[tokio::test]
async fn api_state_on_a_fresh_home_uses_the_defaults() {
    let reply = get("/api/state").await;
    reply.checked("GET", "/api/state");
    assert_eq!(reply.json()["units"], "INCHES");
    assert_eq!(reply.json()["macros"][1]["title"], "Macro 2");
}

#[tokio::test]
async fn unknown_api_paths_answer_json_404s() {
    let reply = get("/api/nope").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.json(), json!({"error": "Not found"}));
    let html = get("/nowhere").await;
    assert_eq!(html.status, StatusCode::NOT_FOUND);
    assert!(html.content_type.starts_with("text/html"));
}

#[tokio::test]
async fn a_wrong_method_is_refused() {
    let reply = request_with(settings(None).1, "POST", "/api/state").await;
    assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn the_websocket_endpoint_needs_an_upgrade_and_a_known_namespace() {
    assert_eq!(get("/ws?ns=%2FMaslowCNC").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(get("/ws?ns=%2FWebMCP").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(get("/ws").await.status, StatusCode::BAD_REQUEST);
}

/// Endpoints this milestone serves; each later milestone adds its own until it equals the contract.
const ROUTED: &[(&str, &str)] = &[("GET", "/api/state"), ("GET", "/next/{path}"), ("GET", "/static/{filename}")];

fn concrete(path: &str) -> String {
    path.replace("{path}", "x").replace("{filename}", "images/GPIO/RPI_pin_layout.svg")
}

#[tokio::test]
async fn every_endpoint_listed_as_routed_is_in_the_contract_and_answers() {
    for (method, path) in ROUTED {
        let entry = Contract::shared().endpoint(method, &concrete(path)).expect("in the contract");
        assert_eq!(entry.path, *path);
        let reply = request_with(settings(None).1, method, &concrete(path)).await;
        assert_ne!(reply.status, StatusCode::METHOD_NOT_ALLOWED, "{method} {path}");
        assert!(reply.status.is_success(), "{method} {path}: {}", reply.status);
    }
}
