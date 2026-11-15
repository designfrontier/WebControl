//! The machine's reach into the computer it runs on: serial ports, the serial log file, CPU load, power-off.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;
use wc_config::{Paths, Settings};
use wc_machine::{FirmwareJob, Host, JobResult};

use crate::firmware::{Avrdude, BACKUPS, Flasher, run_job};

/// `install-pi.sh` allows exactly this command without a password (`/etc/sudoers.d/webcontrol-rs`).
pub const POWER_OFF: [&str; 4] = ["sudo", "--non-interactive", "/usr/bin/systemctl", "poweroff"];
pub const LOG_FILE: &str = "webcontrol-rs.log";
/// The log rotates to `.1` past this size, so months of status reports cannot fill the card.
pub const LOG_LIMIT: u64 = 5 * 1024 * 1024;

/// `actions.updatePorts`: `/dev/tty[A-Za-z]*` on Linux, `/dev/tty.*` on macOS, sorted.
pub fn list_ports(dev: &Path, macos: bool) -> Vec<String> {
    let wanted = |name: &str| match macos {
        true => name.starts_with("tty."),
        false => name.strip_prefix("tty").and_then(|rest| rest.chars().next()).is_some_and(|c| c.is_ascii_alphabetic()),
    };
    let mut ports: Vec<String> = std::fs::read_dir(dev)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| wanted(name))
        .map(|name| dev.join(name).to_string_lossy().into_owned())
        .collect();
    ports.sort();
    ports
}

/// Busy and total jiffies per core, from `/proc/stat`'s `cpuN` lines.
pub fn core_times(stat: &str) -> Vec<(u64, u64)> {
    stat.lines()
        .filter(|line| line.starts_with("cpu") && line.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        .map(|line| {
            let fields: Vec<u64> = line.split_whitespace().skip(1).take(8).filter_map(|f| f.parse().ok()).collect();
            let total: u64 = fields.iter().sum();
            let idle = fields.get(3).copied().unwrap_or(0) + fields.get(4).copied().unwrap_or(0);
            (total - idle, total)
        })
        .collect()
}

/// `max(psutil.cpu_percent(percpu=True))` between two samples.
pub fn busiest_core(before: &[(u64, u64)], after: &[(u64, u64)]) -> f64 {
    before
        .iter()
        .zip(after)
        .map(|((busy0, total0), (busy1, total1))| match total1.saturating_sub(*total0) {
            0 => 0.0,
            span => 100.0 * busy1.saturating_sub(*busy0) as f64 / span as f64,
        })
        .fold(0.0, f64::max)
}

enum LogCommand {
    Append(String),
    Clear,
    Flush(Sender<()>),
}

fn append(path: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() >= LOG_LIMIT) {
        std::fs::rename(path, path.with_extension("log.1"))?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    OpenOptions::new().create(true).append(true).open(path)?.write_all(text.as_bytes())
}

fn clear(path: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path.with_extension("log.1"));
    File::create(path).map(drop)
}

/// The serial log, written on its own thread so a slow card never stalls the machine task.
fn log_writer(path: PathBuf) -> Sender<LogCommand> {
    let (sender, commands) = channel::<LogCommand>();
    std::thread::spawn(move || {
        let mut failed = false;
        commands.iter().for_each(|command| {
            let result = match command {
                LogCommand::Append(text) => append(&path, &text),
                LogCommand::Clear => clear(&path),
                LogCommand::Flush(done) => done.send(()).map_err(std::io::Error::other),
            };
            if let (Err(error), false) = (&result, failed) {
                tracing::warn!(%error, path = %path.display(), "serial log write failed");
            }
            failed = result.is_err();
        });
    });
    sender
}

pub struct SystemHost {
    dev: PathBuf,
    log: Mutex<Sender<LogCommand>>,
    stat: PathBuf,
    cores: Mutex<Vec<(u64, u64)>>,
    power_off: Vec<String>,
    exit: Arc<Notify>,
    settings: Option<Paths>,
    backups: PathBuf,
    flasher: Box<dyn Flasher>,
}

impl SystemHost {
    /// Logs to `<dir>/webcontrol-rs.log`; `exit` is notified when the UI asks WebControl to shut down.
    pub fn new(dir: &Path, exit: Arc<Notify>) -> SystemHost {
        SystemHost {
            dev: "/dev".into(),
            log: Mutex::new(log_writer(dir.join(LOG_FILE))),
            stat: "/proc/stat".into(),
            cores: Mutex::new(Vec::new()),
            power_off: POWER_OFF.map(String::from).to_vec(),
            exit,
            settings: None,
            backups: dir.join(BACKUPS),
            flasher: Box::new(Avrdude::default()),
        }
    }

    /// Flashes with `flasher` in place of avrdude.
    pub fn with_flasher(self, flasher: Box<dyn Flasher>) -> SystemHost {
        SystemHost { flasher, ..self }
    }

    /// Saves the settings under `paths`; without it they stay in memory.
    pub fn saving_to(self, paths: Paths) -> SystemHost {
        SystemHost { settings: Some(paths), ..self }
    }

    /// For tests: other places to find ports and CPU times, and another power-off command.
    pub fn with(self, dev: &Path, stat: &Path, power_off: &[&str]) -> SystemHost {
        SystemHost {
            dev: dev.into(),
            stat: stat.into(),
            power_off: power_off.iter().map(|s| s.to_string()).collect(),
            ..self
        }
    }

    fn send(&self, command: LogCommand) {
        let _ = self.log.lock().expect("log lock").send(command);
    }

    /// Waits until every log line sent so far is written.
    pub fn flush_log(&self) {
        let (done, wait) = channel();
        self.send(LogCommand::Flush(done));
        let _ = wait.recv();
    }
}

impl Host for SystemHost {
    fn list_ports(&self) -> Vec<String> {
        list_ports(&self.dev, cfg!(target_os = "macos"))
    }

    fn log(&self, line: &str) {
        self.send(LogCommand::Append(line.to_string()));
    }

    fn clear_logs(&self) {
        self.send(LogCommand::Clear);
    }

    fn power_off(&self) {
        tracing::warn!(command = ?self.power_off, "powering off");
        let spawned = Command::new(&self.power_off[0]).args(&self.power_off[1..]).spawn();
        match spawned {
            Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
            Err(error) => tracing::error!(%error, "could not run the power-off command"),
        }
    }

    fn exit(&self) {
        self.exit.notify_one();
    }

    fn save_settings(&self, settings: &Settings) {
        let Some(paths) = &self.settings else {
            return;
        };
        if let Err(error) = settings.save(paths) {
            tracing::error!(%error, file = %paths.settings_file().display(), "could not save the settings");
        }
    }

    fn firmware(&self, job: FirmwareJob) -> JobResult {
        run_job(&self.backups, self.flasher.as_ref(), job)
    }

    fn cpu_load(&self) -> f64 {
        let after = std::fs::read_to_string(&self.stat).map(|text| core_times(&text)).unwrap_or_default();
        let before = std::mem::replace(&mut *self.cores.lock().expect("cpu lock"), after.clone());
        busiest_core(&before, &after)
    }
}
