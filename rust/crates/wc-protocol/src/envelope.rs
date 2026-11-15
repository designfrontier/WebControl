use base64::Engine;
use serde::Serialize;
use serde_json::Value;

/// A `/MaslowCNC` message command's decoded `data`.
pub trait Message: Serialize {
    const COMMAND: &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DataFormat {
    Json,
    Int,
    Base64,
}

/// The `message` envelope. Only these constructors build one, so every push has its command's `dataFormat`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub command: String,
    pub data: Value,
    pub data_format: DataFormat,
}

impl Envelope {
    /// `data` is the message as JSON text (wart 1).
    pub fn json<M: Message>(message: &M) -> Envelope {
        let text = serde_json::to_string(message).expect("protocol types always serialise");
        Envelope { command: M::COMMAND.into(), data: Value::String(text), data_format: DataFormat::Json }
    }

    pub fn int(command: &str, value: i64) -> Envelope {
        Envelope { command: command.into(), data: value.into(), data_format: DataFormat::Int }
    }

    /// `None` (nothing loaded) is sent as `""`.
    pub fn base64(command: &str, bytes: Option<&[u8]>) -> Envelope {
        let text = bytes.map(|b| base64::engine::general_purpose::STANDARD.encode(b)).unwrap_or_default();
        Envelope { command: command.into(), data: Value::String(text), data_format: DataFormat::Base64 }
    }
}
