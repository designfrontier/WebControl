use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Envelope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    MaslowCnc,
    Logs,
}

impl Namespace {
    pub fn path(self) -> &'static str {
        match self {
            Namespace::MaslowCnc => "/MaslowCNC",
            Namespace::Logs => "/MaslowCNCLogs",
        }
    }
}

impl FromStr for Namespace {
    type Err = String;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        match path {
            "/MaslowCNC" => Ok(Namespace::MaslowCnc),
            "/MaslowCNCLogs" => Ok(Namespace::Logs),
            other => Err(format!("unknown namespace {other}")),
        }
    }
}

/// One text frame of the plain-WebSocket transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub ns: String,
    pub event: String,
    pub data: Value,
}

impl Frame {
    /// The server's first frame on every connection.
    pub fn connect(ns: Namespace, sid: &str) -> Frame {
        Frame { ns: ns.path().into(), event: "connect".into(), data: json!({ "sid": sid }) }
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("frames always serialise")
    }

    /// `None` for anything that is not a frame; the transport ignores those.
    pub fn decode(text: &str) -> Option<Frame> {
        serde_json::from_str(text).ok()
    }
}

/// A server push: broadcast to every socket of its namespace, or meant for the one socket `client` names.
#[derive(Debug, Clone, PartialEq)]
pub struct Push {
    pub ns: Namespace,
    pub client: Option<String>,
    pub data: Value,
}

impl Push {
    pub fn message(envelope: Envelope) -> Push {
        let data = serde_json::to_value(envelope).expect("envelopes always serialise");
        Push { ns: Namespace::MaslowCnc, client: None, data }
    }

    /// A `/MaslowCNCLogs` push, whose payload is the log object itself.
    pub fn log(data: Value) -> Push {
        Push { ns: Namespace::Logs, client: None, data }
    }

    pub fn to_client(self, sid: &str) -> Push {
        Push { client: Some(sid.into()), ..self }
    }

    pub fn frame(&self) -> Frame {
        Frame { ns: self.ns.path().into(), event: "message".into(), data: self.data.clone() }
    }
}
