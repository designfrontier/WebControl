//! The serial transcript format of `contract/serial/transcript.schema.json`, as serde types.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transcript {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub format_version: u32,
    pub meta: Meta,
    pub setup: Setup,
    #[serde(default)]
    pub timing: Timing,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Meta {
    pub name: String,
    pub description: String,
    pub firmware: Firmware,
    pub webcontrol_commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Firmware {
    pub variant: String,
    pub version: String,
    #[serde(default)]
    pub build: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Start {
    #[default]
    Connected,
    Boot,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Setup {
    #[serde(default)]
    pub start: Start,
    #[serde(default)]
    pub settings: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    pub units: Option<String>,
    #[serde(default)]
    pub units_z: Option<String>,
    #[serde(default)]
    pub home: Option<Point2>,
    #[serde(default)]
    pub gcode: Option<Gcode>,
    #[serde(default)]
    pub gcode_index: u64,
    #[serde(default)]
    pub sled: Option<Point3>,
    #[serde(default)]
    pub boot_banner: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Gcode {
    Lines { lines: Vec<String> },
    Path { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Timing {
    #[serde(default = "Timing::default_quiet")]
    pub quiet: f64,
    #[serde(default = "Timing::default_tx_timeout")]
    pub tx_timeout: f64,
    #[serde(default = "Timing::default_settle")]
    pub settle: f64,
}

impl Timing {
    fn default_quiet() -> f64 {
        0.1
    }
    fn default_tx_timeout() -> f64 {
        2.0
    }
    fn default_settle() -> f64 {
        1.0
    }
}

impl Default for Timing {
    fn default() -> Self {
        Timing { quiet: Self::default_quiet(), tx_timeout: Self::default_tx_timeout(), settle: Self::default_settle() }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiAction {
    pub command: String,
    pub arg: Value,
    pub arg1: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Event {
    Ui {
        action: UiAction,
        #[serde(default)]
        comment: Option<String>,
    },
    Http {
        method: String,
        path: String,
        #[serde(default)]
        json: Option<Value>,
        #[serde(default)]
        form: Option<BTreeMap<String, String>>,
        #[serde(default)]
        status: Option<u16>,
        #[serde(default)]
        comment: Option<String>,
    },
    Rx {
        line: String,
        #[serde(default)]
        comment: Option<String>,
    },
    Tx {
        bytes: String,
        #[serde(default)]
        realtime: bool,
        #[serde(default)]
        timeout: Option<f64>,
        #[serde(default)]
        comment: Option<String>,
    },
    Tick {
        seconds: f64,
        #[serde(default)]
        comment: Option<String>,
    },
    Expect(Box<Expect>),
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub state: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    pub settings: Option<Vec<ExpectSetting>>,
    #[serde(default)]
    pub emitted: Option<Vec<Emitted>>,
    #[serde(default)]
    pub not_emitted: Option<Vec<Emitted>>,
    #[serde(default)]
    pub alerts: Option<Vec<Alert>>,
    #[serde(default)]
    pub no_alerts: Option<Vec<Alert>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectSetting {
    pub section: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Emitted {
    pub command: String,
    #[serde(default)]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alert {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// `contract/serial/`.
pub fn serial_dir() -> PathBuf {
    crate::repo_root().join("contract/serial")
}

impl Transcript {
    pub fn parse(text: &str) -> Result<Transcript, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Transcript, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Transcript::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn directory(name: &str) -> Vec<(PathBuf, Transcript)> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(serial_dir().join(name))
            .expect("transcripts directory")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        paths.into_iter().map(|path| (path.clone(), Transcript::load(&path).unwrap())).collect()
    }

    /// Every transcript in `contract/serial/transcripts/`, sorted by file name.
    pub fn corpus() -> Vec<(PathBuf, Transcript)> {
        Transcript::directory("transcripts")
    }

    /// `contract/serial/transcripts-rust/`: where the Rust backend must differ from Python (the README's known
    /// warts), a transcript there replaces the shared one of the same name; one with a new name is Rust only.
    pub fn overrides() -> Vec<(PathBuf, Transcript)> {
        Transcript::directory("transcripts-rust")
    }

    /// The corpus the Rust backend replays: the shared transcripts with the Rust ones in place of theirs, plus the
    /// Rust-only ones.
    pub fn rust_corpus() -> Vec<(PathBuf, Transcript)> {
        let overrides = Transcript::overrides();
        let shared = Transcript::corpus()
            .into_iter()
            .filter(|(_, t)| !overrides.iter().any(|(_, o)| o.meta.name == t.meta.name));
        let mut all: Vec<(PathBuf, Transcript)> = shared.chain(overrides.clone()).collect();
        all.sort_by(|a, b| a.1.meta.name.cmp(&b.1.meta.name));
        all
    }

    /// The file lines `setup.gcode` loads, before the loader normalises them.
    pub fn gcode_lines(&self) -> Option<Vec<String>> {
        match self.setup.gcode.as_ref()? {
            Gcode::Lines { lines } => Some(lines.clone()),
            Gcode::Path { path } => {
                let text = std::fs::read_to_string(serial_dir().join(path)).expect("transcript gcode file");
                Some(text.lines().map(String::from).collect())
            }
        }
    }
}
