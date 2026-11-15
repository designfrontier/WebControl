use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use wc_config::{Paths, Settings};
use wc_machine::{Event, Machine, NoHost, NoPort, spawn};
use wc_server::{AppState, PingConfig, Platform, router};
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
    request_machine(Machine::new("10.0.0.5:5000", settings), method, path).await
}

async fn request_machine(machine: Machine, method: &str, path: &str) -> Reply {
    let (machine, _task) = spawn(machine, Box::new(NoPort), Arc::new(NoHost));
    let platform = Platform { name: "RPI".into(), can_turn_off: true };
    let home = Paths::new(std::path::Path::new("/nonexistent-home"));
    let app = router(AppState::new(machine, PingConfig::default(), home).on(platform));
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
            "manualZAxisAdjust": false, "probing": false, "hostAddress": "10.0.0.5:5000", "controllerFirmwareVersion": 0.0,
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

/// GET endpoints served so far (the writes are in tests/settings.rs); each later milestone adds its own until it equals the contract.
const ROUTED: &[(&str, &str)] = &[
    ("GET", "/api/state"),
    ("GET", "/api/main"),
    ("GET", "/api/viewer"),
    ("GET", "/api/settings/{sectionId}"),
    ("GET", "/api/system/info"),
    ("GET", "/api/machine/setup"),
    ("GET", "/api/machine/quick-configure"),
    ("GET", "/api/system/pid"),
    ("GET", "/api/calibration/triangular"),
    ("GET", "/api/calibration/holey"),
    ("GET", "/next/{path}"),
    ("GET", "/static/{filename}"),
    ("GET", "/api/files/gcode"),
    ("POST", "/api/files/gcode/directories"),
    ("GET", "/api/files/gcode/loaded"),
    ("GET", "/api/files/gcode/text"),
    ("GET", "/api/files/gcode/custom"),
    ("GET", "/api/boards/files"),
    ("POST", "/api/boards/directories"),
    ("GET", "/api/boards/current"),
    ("POST", "/openGCode"),
    ("POST", "/uploadGCode"),
    ("POST", "/saveGCode"),
    ("POST", "/editGCode"),
    ("POST", "/sendGCode"),
    ("POST", "/importFile"),
    ("POST", "/importFileWCJSON"),
    ("POST", "/importRestoreWebControl"),
    ("POST", "/openBoard"),
    ("POST", "/saveBoard"),
    ("POST", "/editBoard"),
    ("POST", "/trimBoard"),
    ("GET", "/downloadDiagnostics"),
    ("GET", "/backupWebControl"),
];

fn concrete(path: &str) -> String {
    path.replace("{path}", "x").replace("{filename}", "images/GPIO/RPI_pin_layout.svg").replace("{sectionId}", "maslow")
}

#[tokio::test]
async fn every_endpoint_listed_as_routed_is_in_the_contract_and_answers() {
    for (method, path) in ROUTED {
        let entry = Contract::shared().endpoint(method, &concrete(path)).expect("in the contract");
        assert_eq!(entry.path, *path);
        let reply = request_with(settings(None).1, method, &concrete(path)).await;
        assert_ne!(reply.status, StatusCode::METHOD_NOT_ALLOWED, "{method} {path}");
        assert_ne!(reply.status, StatusCode::NOT_FOUND, "{method} {path}");
    }
}

/// A machine that has seen a 51.292 controller boot, before the shell takes it.
fn on_51_292(settings: Settings) -> Machine {
    let mut machine = Machine::new("10.0.0.5:5000", settings);
    let now = std::time::Duration::ZERO;
    machine.handle(Event::PortOpened { port: "/dev/ttyACM0".into() }, now);
    ["Firmware Version 51.29", "Change in setting, rewriting EEPROM"].iter().for_each(|line| {
        machine.handle(Event::ControllerLine(line.to_string()), now);
    });
    machine
}

/// Ported from test_api_main.
#[tokio::test]
async fn api_main_reports_the_run_status_and_the_touch_plate() {
    let file = json!({"Advanced Settings": [{"key": "touchPlate", "value": "1"}]});
    let reply = request_with(settings(Some(file)).1, "GET", "/api/main").await;
    reply.checked("GET", "/api/main");
    assert_eq!(
        reply.json(),
        json!({"status": {"uploadFlag": 0, "positioningMode": 0, "currentTool": 0, "jobInProgress": false,
            "manualZAxisAdjust": false, "probing": false}, "touchPlate": true, "gcode": {"lineIndex": 0, "line": "", "lineCount": 0}})
    );
    let flags =
        [(json!(0), false), (json!(true), true), (json!("True"), true), (json!("2.5"), true), (json!("no"), false)];
    for (value, expected) in flags {
        let file = json!({"Advanced Settings": [{"key": "touchPlate", "value": value}]});
        let reply = request_with(settings(Some(file)).1, "GET", "/api/main").await;
        assert_eq!(reply.json()["touchPlate"], expected, "{value}");
    }
}

#[tokio::test]
async fn api_main_shows_the_loaded_files_current_line() {
    let mut machine = Machine::new("h", Settings::defaults());
    machine.load_gcode("G0 X1\nG0 X2\n");
    let reply = request_machine(machine, "GET", "/api/main").await;
    assert_eq!(reply.json()["gcode"], json!({"lineIndex": 0, "line": "G0 X1 ", "lineCount": 3}));
}

/// Ported from test_api_viewer: the frame in inches, no gcode yet, and the bed-sized board.
#[tokio::test]
async fn api_viewer_reports_the_frame_in_inches() {
    let file = json!({"Maslow Settings": [{"key": "bedWidth", "value": "2438.4"}, {"key": "bedHeight", "value": 1219.2},
        {"key": "motorOffsetY", "value": "x"}, {"key": "motorSpacingX", "value": "3601.2"}]});
    let reply = request_with(settings(Some(file)).1, "GET", "/api/viewer").await;
    reply.checked("GET", "/api/viewer");
    assert_eq!(
        reply.json(),
        json!({"machine": {"bedWidth": 96.0, "bedHeight": 48.0, "motorOffsetY": null, "motorSpacingX": 141.7795},
            "gcode": null, "board": {"width": 96.0, "height": 48.0, "thickness": 0.75, "centerX": 0.0, "centerY": 0.0,
                "boardID": "-", "material": "-", "fileName": ""}, "cutData": "H4sIAAAAAAAA/4uOBQApu0wNAgAAAA=="})
    );
}

/// Ported from test_api_settings: each section as a form, GPIO pins offered the action list.
#[tokio::test]
async fn api_settings_describes_each_section() {
    for id in ["maslow", "advanced", "webcontrol", "camera", "gpio"] {
        let path = format!("/api/settings/{id}");
        let reply = get(&path).await;
        reply.checked("GET", &path);
        assert_eq!(reply.json()["id"], id);
        assert_eq!(reply.json()["enableCustom"], false);
        assert_eq!(reply.json().get("ports").is_some(), id == "maslow", "{id}");
    }
    let maslow = get("/api/settings/maslow").await.json();
    let field = |key: &str| maslow["settings"].as_array().unwrap().iter().find(|s| s["key"] == key).unwrap().clone();
    assert_eq!(field("bedWidth")["numeric"], true);
    assert_eq!(field("COMport")["numeric"], false);
    assert_eq!(field("bufferOn")["type"], "bool");
    let advanced = get("/api/settings/advanced").await.json();
    let spindle = advanced["settings"].as_array().unwrap().iter().find(|s| s["key"] == "spindleAutomate").unwrap();
    assert_eq!(spindle["options"], json!(["None", "Servo", "Relay_High", "Relay_Low"]));
    let gpio = get("/api/settings/gpio").await.json();
    let options = &gpio["settings"][0]["options"];
    assert_eq!((options[0].clone(), options[1].clone()), (json!(""), json!("Spindle On")));
}

#[tokio::test]
async fn api_settings_marks_blank_numbers_numeric_and_keeps_unknown_fields_out() {
    let file = json!({"Advanced Settings": [
        {"key": "homeX", "value": "", "default": "0", "type": "string", "custom": 1, "extra": true},
        {"key": "odd", "value": 3}]});
    let advanced = request_with(settings(Some(file)).1, "GET", "/api/settings/advanced").await;
    advanced.checked("GET", "/api/settings/advanced");
    let list = advanced.json()["settings"].clone();
    assert_eq!(
        list[0],
        json!({"key": "homeX", "title": "homeX", "desc": "", "type": "string", "value": "",
        "custom": true, "numeric": true})
    );
    assert_eq!(list[1]["type"], "string");
    assert_eq!(list[1]["numeric"], true);
}

#[tokio::test]
async fn api_settings_for_an_unknown_section_is_a_json_404() {
    let reply = get("/api/settings/nope").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.json(), json!({"error": "Not found"}));
}

/// Ported from test_api_system's info half.
#[tokio::test]
async fn api_system_info_reports_the_controller_and_platform() {
    let reply = get("/api/system/info").await;
    reply.checked("GET", "/api/system/info");
    let info = reply.json();
    assert_eq!((info["platform"].clone(), info["canTurnOff"].clone()), (json!("RPI"), json!(true)));
    assert_eq!(
        (info["firmwareBuild"].clone(), info["firmwareSupportsZaxisLimit"].clone()),
        (json!(null), json!(false))
    );
    assert_eq!(info["update"], json!({"available": false, "version": null}));
    assert_eq!(
        (info["firmwareUpgrade"]["target"].clone(), info["firmwareUpgrade"]["stage"].clone()),
        (json!("51.292"), json!("idle"))
    );
    let upgraded = request_machine(on_51_292(Settings::defaults()), "GET", "/api/system/info").await.json();
    assert_eq!(upgraded["controllerFirmwareVersion"], 51.29);
    assert_eq!(
        (upgraded["firmwareBuild"].clone(), upgraded["firmwareSupportsZaxisLimit"].clone()),
        (json!("51.292"), json!(true))
    );
}

#[tokio::test]
async fn the_state_follows_the_machine() {
    let state = request_machine(on_51_292(Settings::defaults()), "GET", "/api/state").await;
    state.checked("GET", "/api/state");
    assert_eq!(state.json()["firmwareBuild"], "51.292");
}

#[test]
fn the_platform_is_the_pi_only_on_linux_arm() {
    let platform = Platform::current();
    assert_eq!(platform.can_turn_off, cfg!(target_os = "linux"));
    assert_eq!(platform.name == "RPI", cfg!(all(target_os = "linux", target_arch = "aarch64")));
}

#[tokio::test]
async fn help_serves_the_index_page_by_default_without_its_front_matter() {
    let reply = get("/api/system/help").await;
    reply.checked("GET", "/api/system/help");
    assert_eq!(reply.status, StatusCode::OK);
    let page = reply.json();
    assert_eq!((page["path"].as_str(), page["title"].as_str()), (Some("index.md"), Some("Home")));
    assert!(page["markdown"].as_str().unwrap().starts_with("# WebControl\n"), "{page}");
}

/// tests/fixtures/help/python-pages.json holds Python's title and a SHA-256 of its markdown for every docs/ page.
#[tokio::test]
async fn every_help_page_matches_what_python_serves() {
    use sha2::Digest;
    let fixture = wc_testkit::repo_root().join("tests/fixtures/help/python-pages.json");
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    for (path, want) in expected.as_object().unwrap() {
        let uri = format!("/api/system/help?path={}", path.replace('&', "%26"));
        let reply = get(&uri).await;
        reply.checked("GET", "/api/system/help");
        let page = reply.json();
        let digest = format!("{:x}", sha2::Sha256::digest(page["markdown"].as_str().unwrap().as_bytes()));
        assert_eq!((page["path"].as_str().unwrap(), &page["title"]), (path.as_str(), &want["title"]));
        assert_eq!(digest, want["sha256"].as_str().unwrap(), "{path}");
    }
}

#[tokio::test]
async fn help_refuses_pages_outside_docs_and_files_that_are_not_markdown() {
    for path in ["../README.md", "%2e%2e/README.md", "/etc/passwd", "_config.yml", "nope.md", "assets", "a%00.md"] {
        let reply = get(&format!("/api/system/help?path={path}")).await;
        reply.checked("GET", "/api/system/help");
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(reply.json(), json!({"error": "no such help page"}), "{path}");
    }
    let dotted = get("/api/system/help?path=./GettingStarted/../index.md").await;
    assert_eq!(dotted.status, StatusCode::NOT_FOUND, "Python refuses any .. segment");
}

#[tokio::test]
async fn help_images_are_served_from_docs_assets() {
    let path = "/api/system/help/assets/Screenshot.png";
    let reply = get(path).await;
    reply.checked("GET", path);
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.content_type, "image/png");
    assert!(reply.body.starts_with(b"\x89PNG"));
    let nested = get("/api/system/help/assets/boards/boardCut.png").await;
    assert_eq!((nested.status, nested.content_type.as_str()), (StatusCode::OK, "image/png"));
    for missing in ["/api/system/help/assets/nope.png", "/api/system/help/assets/../index.md"] {
        let reply = get(missing).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{missing}");
        assert!(reply.content_type.starts_with("text/html"), "{missing}");
    }
}

#[test]
fn front_matter_and_jekyll_markup_are_stripped_as_python_does() {
    use wc_server::help::{clean_markdown, front_matter};
    let page = "\n---\nlayout: default\ntitle: 'Quoted'\n---\n\nBody\n---\nmore\n\n";
    assert_eq!(front_matter(page), (Some("Quoted".into()), "Body\n---\nmore".into()));
    assert_eq!(front_matter("no front matter\n"), (None, "no front matter".into()));
    assert_eq!(front_matter("---\ntitle: x\nunclosed"), (None, "---\ntitle: x\nunclosed".into()));
    assert_eq!(front_matter("---\nnav_order: 1\n---\nx"), (None, "x".into()));
    let jekyll = "[a]({{ site.baseurl }}{% link Actions/x.md %}) New{: .label .label-blue }Â</br>\
        <span class=\"k\">kept</span> <img src=\"../assets/y.png\" alt=\"Y\" width=\"5\">";
    assert_eq!(clean_markdown(jekyll), "[a](Actions/x.md) New  kept ![Y](../assets/y.png)");
}
