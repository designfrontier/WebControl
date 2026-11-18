//! A throwaway home with the file routes in front of a real machine task, every response and request body
//! checked against the contract.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Map, Value, json};
use tokio::sync::Notify;
use wc_config::{Paths, Settings};
use wc_machine::{Event, Machine, MachineHandle, NoPort, spawn};
use wc_protocol::Action;
use wc_server::host::SystemHost;
use wc_server::{AppState, PingConfig, router};
use wc_testkit::contract::{Contract, ResponseBody};

pub struct Reply {
    pub status: StatusCode,
    pub content_type: String,
    pub disposition: Option<String>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

pub struct Server {
    pub app: Router,
    pub home: tempfile::TempDir,
    pub machine: MachineHandle,
    pub host: Arc<SystemHost>,
}

pub enum Part<'a> {
    Field(&'a str, &'a str),
    File(&'a str, &'a str, &'a [u8]),
}

const BOUNDARY: &str = "wc-boundary";

fn multipart(parts: &[Part]) -> Vec<u8> {
    let mut body = Vec::new();
    parts.iter().for_each(|part| {
        body.extend(format!("--{BOUNDARY}\r\n").bytes());
        match part {
            Part::Field(name, value) => {
                body.extend(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").bytes());
            }
            Part::File(name, file, bytes) => {
                let head = format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{file}\"\r\n\
                     Content-Type: application/octet-stream\r\n\r\n"
                );
                body.extend(head.bytes());
                body.extend(*bytes);
                body.extend(b"\r\n");
            }
        }
    });
    body.extend(format!("--{BOUNDARY}--\r\n").bytes());
    body
}

/// The form as the contract's request schema sees it: text fields, and files by name.
fn form_json(parts: &[Part]) -> Value {
    let mut map = Map::new();
    parts.iter().for_each(|part| match part {
        Part::Field(name, value) => {
            map.insert(name.to_string(), json!(value));
        }
        Part::File(name, file, _) if name.ends_with("[]") => {
            let list = map.entry(name.to_string()).or_insert_with(|| json!([]));
            list.as_array_mut().unwrap().push(json!(file));
        }
        Part::File(name, file, _) => {
            map.insert(name.to_string(), json!(file));
        }
    });
    Value::Object(map)
}

/// A FIFO at `path`: a read of it waits until something writes.
pub fn fifo(path: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o644) }, 0);
}

pub fn machine_with(settings: Settings) -> Machine {
    Machine::new("10.0.0.5:5000", settings)
}

impl Server {
    pub fn new() -> Server {
        Server::with(machine_with(Settings::defaults()))
    }

    pub fn with(machine: Machine) -> Server {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::new(home.path());
        [paths.gcode_dir(), paths.boards_dir(), paths.imports_dir()]
            .iter()
            .for_each(|d| std::fs::create_dir_all(d).unwrap());
        machine.snapshot().settings.save(&paths).unwrap();
        let host = Arc::new(SystemHost::new(&paths.dir(), Arc::new(Notify::new())).saving_to(paths.clone()));
        let machine = machine.with_library(paths.gcode_dir());
        let (machine, _task) = spawn(machine, Box::new(NoPort), host.clone());
        let app = router(AppState::new(machine.clone(), PingConfig::default(), paths));
        Server { app, home, machine, host }
    }

    pub fn dir(&self) -> PathBuf {
        self.home.path().join(".WebControl")
    }

    pub fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.dir().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    pub async fn call(&self, method: &str, path: &str, content_type: Option<&str>, body: Vec<u8>) -> Reply {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        let response =
            tower::ServiceExt::oneshot(self.app.clone(), request.body(Body::from(body)).unwrap()).await.unwrap();
        let text = |name| response.headers().get(name).map(|v: &header::HeaderValue| v.to_str().unwrap().to_string());
        let (content_type, disposition) =
            (text(header::CONTENT_TYPE).unwrap_or_default(), text(header::CONTENT_DISPOSITION));
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes().to_vec();
        Reply { status, content_type, disposition, body }
    }

    /// A response the contract lists, checked against its schema.
    fn checked(method: &str, path: &str, reply: Reply) -> Reply {
        let route = path.split('?').next().unwrap();
        // The contract notes that a missing form field gives Werkzeug's HTML 400 even where its 400 is JSON.
        if reply.status == StatusCode::BAD_REQUEST && reply.content_type.starts_with("text/html") {
            assert!(String::from_utf8_lossy(&reply.body).contains("<title>400 Bad Request</title>"));
            return reply;
        }
        let body = match reply.content_type.starts_with("application/json") {
            true => ResponseBody::Json(reply.json()),
            false => ResponseBody::Other(reply.content_type.clone()),
        };
        Contract::shared().validate_response(method, route, reply.status.as_u16(), &body).unwrap();
        reply
    }

    pub async fn get(&self, path: &str) -> Reply {
        Server::checked("GET", path, self.call("GET", path, None, Vec::new()).await)
    }

    pub async fn post_json(&self, path: &str, body: Value) -> Reply {
        self.send_json("POST", path, body).await
    }

    pub async fn send_json(&self, method: &str, path: &str, body: Value) -> Reply {
        Contract::shared().validate_request(method, path, &body).unwrap();
        let body = body.to_string().into_bytes();
        Server::checked(method, path, self.call(method, path, Some("application/json"), body).await)
    }

    pub async fn post_raw(&self, path: &str, content_type: &str, body: &[u8]) -> Reply {
        self.call("POST", path, Some(content_type), body.to_vec()).await
    }

    /// A multipart form whose fields fit the route's request schema.
    pub async fn post_form(&self, path: &str, parts: &[Part<'_>]) -> Reply {
        Contract::shared().validate_request("POST", path, &form_json(parts)).unwrap();
        self.post_unchecked(path, parts).await
    }

    /// A multipart form the contract would refuse, e.g. missing a field.
    pub async fn post_unchecked(&self, path: &str, parts: &[Part<'_>]) -> Reply {
        let content_type = format!("multipart/form-data; boundary={BOUNDARY}");
        Server::checked("POST", path, self.call("POST", path, Some(&content_type), multipart(parts)).await)
    }

    pub fn act(&self, command: &str, arg: Value) {
        self.machine.send(Event::Action(Action::new(command, arg, json!(""))));
    }

    /// Waits for the machine to take everything sent so far, and for its saves to land.
    pub async fn settle(&self) {
        let _ = self.machine.request(wc_machine::files::Request::SetSettings(Vec::new())).await;
        self.host.flush_log();
        let wanted = self.machine.snapshot().settings.to_python_json();
        let file = self.dir().join("webcontrol.json");
        let landed = async {
            while std::fs::read_to_string(&file).ok().as_deref() != Some(wanted.as_str()) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), landed).await.expect("the settings save landed");
    }

    pub fn saved_settings(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.dir().join("webcontrol.json")).unwrap()).unwrap()
    }
}
