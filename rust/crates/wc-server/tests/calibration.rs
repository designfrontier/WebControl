//! M5's routes over HTTP: the calibration parameters, Calculate on a blocking thread, and Accept, each body checked
//! against its closed schema (ported from test_api_calibration and test_calibration_accept).

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use wc_config::Settings;
use wc_machine::{Machine, NoHost, NoPort, spawn};
use wc_server::{AppState, PingConfig, router};
use wc_testkit::contract::{Contract, ResponseBody};

fn app() -> Router {
    let (machine, _task) = spawn(Machine::new("h", Settings::defaults()), Box::new(NoPort), Arc::new(NoHost));
    router(AppState::new(machine, PingConfig::default()))
}

async fn call(app: &Router, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let request = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json");
    let request = request.body(body.as_ref().map_or_else(Body::empty, |b| Body::from(b.to_string()))).unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), request).await.unwrap();
    let status = response.status().as_u16();
    let value: Value = serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let contract = Contract::shared();
    contract.validate_response(method, path, status, &ResponseBody::Json(value.clone())).unwrap();
    if let Some(body) = body.filter(|_| status < 400) {
        contract.validate_request(method, path, &body).unwrap();
    }
    (status, value)
}

#[tokio::test]
async fn the_parameters_are_the_stored_settings() {
    let app = app();
    let (status, triangular) = call(&app, "GET", "/api/calibration/triangular", None).await;
    assert_eq!(status, 200);
    assert_eq!(triangular, json!({"motorYoffset": 463.0, "rotationRadius": 139.1, "chainSagCorrection": 33.0}));
    let (_, holey) = call(&app, "GET", "/api/calibration/holey", None).await;
    assert_eq!(holey["distanceBetweenMotors"], 2978.4);
}

#[tokio::test]
async fn holey_calculates_then_accepts_once() {
    let app = app();
    assert_eq!(call(&app, "POST", "/api/calibration/holey/accept", Some(json!({}))).await.0, 409);
    let measurements = [966.0, 964.5, 965.8, 964.9, 711.6, 710.9, 711.4, 1199.5, 1198.2, 1199.1, 1198.6, 254.3];
    let (status, calculated) =
        call(&app, "POST", "/api/calibration/holey", Some(json!({ "measurements": measurements }))).await;
    assert_eq!(status, 200);
    assert_eq!(calculated["distanceBetweenMotors"], 2978.54418);
    let (status, accepted) = call(&app, "POST", "/api/calibration/holey/accept", Some(json!({}))).await;
    assert_eq!(status, 200);
    assert_eq!(accepted["motorYoffset"], 462.64616);
    let (_, stored) = call(&app, "GET", "/api/calibration/holey", None).await;
    assert_eq!(stored, accepted);
    assert_eq!(call(&app, "POST", "/api/calibration/holey/accept", Some(json!({}))).await.0, 409);
}

#[tokio::test]
async fn triangular_refuses_a_wrong_bit_and_accepts_a_calculation() {
    let app = app();
    let mut body = json!({"cut12": 1930, "cut34": 1930, "cut5": 300, "bitDiameter": 30});
    let (status, refused) = call(&app, "POST", "/api/calibration/triangular", Some(body.clone())).await;
    assert_eq!(status, 400);
    assert!(refused["error"].as_str().unwrap().starts_with("The bit diameter of 30.0 mm seems wrong."));
    assert_eq!(call(&app, "POST", "/api/calibration/triangular", None).await.0, 400);
    body["bitDiameter"] = json!("6.35");
    let (status, calculated) = call(&app, "POST", "/api/calibration/triangular", Some(body)).await;
    assert_eq!(status, 200);
    let (status, accepted) = call(&app, "POST", "/api/calibration/triangular/accept", Some(json!({}))).await;
    assert_eq!(status, 200);
    assert_eq!(accepted["rotationRadius"], calculated["rotationRadius"]);
}
