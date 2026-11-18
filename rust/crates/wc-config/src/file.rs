//! Writing `webcontrol.json` as `Config.saveSettings` does, and the startup `Config.__init__` performs, plus the
//! one-time `.pre-rs` snapshot the Rust backend takes before it first writes the shared file.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::settings::DEFAULTS;
use crate::{Paths, Settings, Source, python_json};

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// A second name for the current file, so it is never missing while the new one is renamed over it.
fn keep_backup(path: &Path, backup: &Path) -> io::Result<()> {
    if let Err(e) = fs::remove_file(backup)
        && e.kind() != io::ErrorKind::NotFound
    {
        return Err(e);
    }
    fs::hard_link(path, backup).or_else(|_| fs::copy(path, backup).map(drop))
}

impl Paths {
    pub fn settings_backup(&self) -> PathBuf {
        with_suffix(&self.settings_file(), ".bak")
    }

    /// The file as it was before the Rust backend first wrote to it.
    pub fn settings_snapshot(&self) -> PathBuf {
        with_suffix(&self.settings_file(), ".pre-rs")
    }
}

impl Settings {
    /// The settings as Python's `json.dump(..., sort_keys=True, indent=4, ensure_ascii=False)` writes them.
    pub fn to_python_json(&self) -> String {
        python_json(&Value::Object(self.sections.clone()))
    }

    /// Atomically: a temp file flushed to disk, the old file kept as `.bak`, then the rename over it.
    pub fn save(&self, paths: &Paths) -> io::Result<()> {
        let path = paths.settings_file();
        let tmp = with_suffix(&path, ".tmp");
        let written = (|| {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(self.to_python_json().as_bytes())?;
            file.sync_all()?;
            if path.exists() {
                keep_backup(&path, &paths.settings_backup())?;
            }
            fs::rename(&tmp, &path)
        })();
        if tmp.exists() {
            let _ = fs::remove_file(&tmp);
        }
        written
    }

    /// `Config.__init__` then `main.py`'s `computeSettings`: the directories, the defaults on a first run, the
    /// fallbacks, the derived settings, and a save when any of those changed the file (an unreadable one kept as
    /// `.corrupt`).
    pub fn open(paths: &Paths) -> io::Result<Settings> {
        [paths.dir(), paths.gcode_dir(), paths.imports_dir(), paths.boards_dir()]
            .iter()
            .try_for_each(fs::create_dir_all)?;
        let file = paths.settings_file();
        if !file.exists() && !paths.settings_backup().exists() {
            fs::write(&file, DEFAULTS)?;
        }
        if file.exists() && !paths.settings_snapshot().exists() {
            fs::copy(&file, paths.settings_snapshot())?;
        }
        let (mut settings, recovered) = Settings::read(paths);
        let added = settings.add_missing_defaults();
        let updated = !settings.compute().is_empty() || added;
        if recovered != Source::File && file.exists() {
            fs::rename(&file, with_suffix(&file, ".corrupt"))?;
        }
        if updated || recovered != Source::File {
            settings.save(paths)?;
        }
        Ok(settings)
    }

    /// The settings `open` would make of a home it cannot prepare, with nothing written.
    pub fn load_derived(paths: &Paths) -> Settings {
        let mut settings = Settings::load(paths);
        settings.compute();
        settings
    }
}
