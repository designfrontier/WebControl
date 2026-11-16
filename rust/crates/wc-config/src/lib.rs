//! `~/.WebControl` as the Python backend lays it out, `webcontrol.json` read and written the way `config/config.py`
//! does, so either backend can run on the same files, and the gcode and board libraries in it.

mod edit;
mod file;
mod json;
pub mod library;
mod python;
mod settings;
mod store;

pub use edit::{Stored, python_eq, python_float};
pub use json::python_json;
pub use python::{parse_number, python_float_str, python_str};
pub use settings::{FirmwareOption, Settings, Source};
pub use store::{Imported, Taken};

use std::path::{Path, PathBuf};

/// Where the shared `~/.WebControl` lives for a given home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    home: PathBuf,
}

impl Paths {
    pub fn new(home: &Path) -> Paths {
        Paths { home: home.to_path_buf() }
    }

    pub fn home(&self) -> PathBuf {
        self.home.clone()
    }

    pub fn dir(&self) -> PathBuf {
        self.home.join(".WebControl")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.dir().join("webcontrol.json")
    }

    pub fn gcode_dir(&self) -> PathBuf {
        self.dir().join("gcode")
    }

    pub fn boards_dir(&self) -> PathBuf {
        self.dir().join("boards")
    }

    pub fn imports_dir(&self) -> PathBuf {
        self.dir().join("imports")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.dir().join("downloads")
    }
}
