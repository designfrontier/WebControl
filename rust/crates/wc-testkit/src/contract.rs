//! Validates what a backend sends and accepts against `contract/`, with every object schema that lists its
//! properties closed, as the Python backend's contract test did.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use jsonschema::Validator;
use regex::Regex;
use serde_json::{Map, Value};

pub type Checked = Result<(), String>;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub method: String,
    pub path: String,
    pub family: String,
    pub body: Option<String>,
    pub responses: BTreeMap<u16, ResponseSpec>,
    pattern: Regex,
    placeholders: usize,
}

#[derive(Debug, Clone)]
pub struct ResponseSpec {
    pub content_type: String,
    pub schema: Option<String>,
}

/// A response as the test saw it: a parsed JSON body, or the content type of anything else.
#[derive(Debug, Clone)]
pub enum ResponseBody {
    Json(Value),
    Other(String),
}

pub struct Contract {
    root: PathBuf,
    index: Value,
    endpoints: Vec<Endpoint>,
    validators: Mutex<HashMap<String, Arc<Validator>>>,
    seen: Mutex<BTreeSet<String>>,
}

/// The schema with every object that lists its properties forbidding others; an explicit
/// `additionalProperties` wins.
pub fn closed(schema: &Value) -> Value {
    match schema {
        Value::Array(items) => Value::Array(items.iter().map(closed).collect()),
        Value::Object(map) => {
            let mut result: Map<String, Value> = map.iter().map(|(k, v)| (k.clone(), closed(v))).collect();
            let lists_properties =
                result.get("type") == Some(&Value::from("object")) && result.contains_key("properties");
            if lists_properties && !result.contains_key("additionalProperties") {
                result.insert("additionalProperties".into(), Value::Bool(false));
            }
            Value::Object(result)
        }
        other => other.clone(),
    }
}

fn template_pattern(path: &str, family: &str) -> (Regex, usize) {
    let placeholder = Regex::new(r"\{[^}]+\}").unwrap();
    let any = if family == "static" { ".*" } else { "[^/]+" };
    let literal: Vec<String> = placeholder.split(path).map(regex::escape).collect();
    (Regex::new(&format!("^{}$", literal.join(any))).unwrap(), literal.len() - 1)
}

fn parse_endpoint(entry: &Value) -> Endpoint {
    let text = |key: &str| entry[key].as_str().unwrap_or_default().to_string();
    let responses = entry["responses"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(status, spec)| {
            let spec = ResponseSpec {
                content_type: spec["contentType"].as_str().unwrap_or_default().to_string(),
                schema: spec["schema"].as_str().map(String::from),
            };
            (status.parse::<u16>().expect("numeric status"), spec)
        })
        .collect();
    let (pattern, placeholders) = template_pattern(&text("path"), &text("family"));
    Endpoint {
        method: text("method"),
        path: text("path"),
        family: text("family"),
        body: entry["body"]["schema"].as_str().map(String::from),
        responses,
        pattern,
        placeholders,
    }
}

fn read_json(path: &PathBuf) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

impl Contract {
    pub fn load() -> Contract {
        let root = crate::repo_root().join("contract");
        let index = read_json(&root.join("index.json"));
        let endpoints = index["http"].as_array().expect("http list").iter().map(parse_endpoint).collect();
        Contract { root, index, endpoints, validators: Mutex::default(), seen: Mutex::default() }
    }

    /// One contract per test binary; use `load` when a test inspects `seen_commands`.
    pub fn shared() -> &'static Contract {
        static SHARED: OnceLock<Contract> = OnceLock::new();
        SHARED.get_or_init(Contract::load)
    }

    pub fn version(&self) -> u32 {
        self.index["version"].as_u64().expect("version") as u32
    }

    pub fn schema(&self, relative: &str) -> Value {
        read_json(&self.root.join(relative))
    }

    fn validator(&self, relative: &str) -> Arc<Validator> {
        let mut cache = self.validators.lock().unwrap();
        cache
            .entry(relative.to_string())
            .or_insert_with(|| {
                let schema = closed(&self.schema(relative));
                Arc::new(jsonschema::draft202012::new(&schema).unwrap_or_else(|e| panic!("{relative}: {e}")))
            })
            .clone()
    }

    /// Checks `value` against one schema file, closed.
    pub fn check(&self, relative: &str, value: &Value) -> Checked {
        let errors: Vec<String> =
            self.validator(relative).iter_errors(value).map(|e| format!("{} at {}", e, e.instance_path())).collect();
        if errors.is_empty() { Ok(()) } else { Err(format!("{relative}: {}", errors.join("; "))) }
    }

    fn command_entry(&self, command: &str) -> Result<&Value, String> {
        let commands = &self.index["socket"]["namespaces"]["/MaslowCNC"]["serverToClient"]["message"]["commands"];
        commands.get(command).ok_or_else(|| format!("{command} is not a serverToClient command"))
    }

    /// The decoded `data` of a `/MaslowCNC` message command.
    pub fn validate_message(&self, command: &str, data: &Value) -> Checked {
        let schema = self.command_entry(command)?["schema"].as_str().expect("schema path");
        self.check(schema, data)?;
        self.seen.lock().unwrap().insert(command.to_string());
        Ok(())
    }

    /// A whole `message` envelope: its shape, its command's `dataFormat`, and the decoded data.
    pub fn validate_envelope(&self, envelope: &Value) -> Checked {
        self.check("schemas/socket/envelope.json", envelope)?;
        let command = envelope["command"].as_str().unwrap_or_default();
        let expected = self.command_entry(command)?["dataFormat"].as_str().unwrap_or("json");
        let actual = envelope.get("dataFormat").and_then(Value::as_str);
        if actual != Some(expected) {
            return Err(format!("{command}: dataFormat {actual:?}, the contract says {expected:?}"));
        }
        match (expected, &envelope["data"]) {
            ("json", Value::String(text)) => {
                let data = serde_json::from_str(text).map_err(|e| format!("{command}: data is not JSON text: {e}"))?;
                self.validate_message(command, &data)
            }
            ("json", other) => Err(format!("{command}: data must be JSON text, got {other}")),
            (_, data) => self.validate_message(command, data),
        }
    }

    /// One text frame of the plain-WebSocket transport, in either direction.
    pub fn validate_frame(&self, text: &str) -> Checked {
        let frame: Value = serde_json::from_str(text).map_err(|e| format!("frame is not JSON: {e}"))?;
        self.check("schemas/websocket/frame.json", &frame)?;
        let (ns, event, data) = (frame["ns"].as_str().unwrap(), frame["event"].as_str().unwrap(), &frame["data"]);
        let namespace = self.index["socket"]["namespaces"].get(ns).ok_or_else(|| format!("unknown namespace {ns}"))?;
        match (ns, event) {
            (_, "connect") => match data.as_object() {
                Some(map) if map.len() == 1 && map.get("sid").is_some_and(Value::is_string) => Ok(()),
                _ => Err(format!("connect handshake must be {{\"sid\": <text>}}, got {data}")),
            },
            ("/MaslowCNC", "message") => self.validate_envelope(data),
            (_, "message") if namespace["serverToClient"]["message"]["schema"].is_string() => {
                self.check(namespace["serverToClient"]["message"]["schema"].as_str().unwrap(), data)
            }
            _ => match namespace["clientToServer"][event]["schema"].as_str() {
                Some(schema) => self.check(schema, data),
                None => Err(format!("{event} is not an event of {ns}")),
            },
        }
    }

    /// The endpoint a concrete request path hits; literal paths win over templates.
    pub fn endpoint(&self, method: &str, path: &str) -> Option<&Endpoint> {
        self.endpoints.iter().filter(|e| e.method == method && e.pattern.is_match(path)).min_by_key(|e| e.placeholders)
    }

    fn require_endpoint(&self, method: &str, path: &str) -> Result<&Endpoint, String> {
        self.endpoint(method, path).ok_or_else(|| format!("{method} {path} is not in contract/index.json"))
    }

    pub fn validate_response(&self, method: &str, path: &str, status: u16, body: &ResponseBody) -> Checked {
        let endpoint = self.require_endpoint(method, path)?;
        let spec = endpoint
            .responses
            .get(&status)
            .ok_or_else(|| format!("{method} {} lists no {status} response", endpoint.path))?;
        match (&spec.schema, body) {
            (Some(schema), ResponseBody::Json(value)) => self.check(schema, value),
            (None, ResponseBody::Other(actual))
                if spec.content_type == "application/octet-stream" || actual.starts_with(&spec.content_type) =>
            {
                Ok(())
            }
            _ => Err(format!("{method} {} {status} must be {}, got {body:?}", endpoint.path, spec.content_type)),
        }
    }

    pub fn validate_request(&self, method: &str, path: &str, body: &Value) -> Checked {
        let endpoint = self.require_endpoint(method, path)?;
        let schema =
            endpoint.body.as_deref().ok_or_else(|| format!("{method} {} takes no request body", endpoint.path))?;
        self.check(schema, body)
    }

    pub fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    pub fn client_events(&self, namespace: &str) -> Vec<String> {
        let events = &self.index["socket"]["namespaces"][namespace]["clientToServer"];
        events.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default()
    }

    pub fn message_commands(&self) -> Vec<String> {
        let commands = &self.index["socket"]["namespaces"]["/MaslowCNC"]["serverToClient"]["message"]["commands"];
        commands.as_object().expect("commands").keys().cloned().collect()
    }

    pub fn actions(&self) -> &Map<String, Value> {
        self.index["actions"].as_object().expect("actions")
    }

    /// The message commands that have passed `validate_message` so far.
    pub fn seen_commands(&self) -> BTreeSet<String> {
        self.seen.lock().unwrap().clone()
    }
}
