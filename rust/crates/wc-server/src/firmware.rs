//! The host's half of the firmware upgrade: backups under `~/.WebControl/firmware-backups/`, and avrdude, or under
//! `--simulate` a stand-in that flashes the simulated controller.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wc_firmware::image::{Image, application_bytes};
use wc_firmware::upgrade::{read_args, write_args};
use wc_machine::{FirmwareJob, JobResult};
use wc_sim::{Build, Sim};

pub const BACKUPS: &str = "firmware-backups";
/// The port the machine just closed must be free before avrdude opens it; Python waited this long too.
pub const PORT_RELEASE: Duration = Duration::from_millis(1500);

/// Reads and writes the controller's flash.
pub trait Flasher: Send + Sync {
    /// The flash as Intel HEX, or what went wrong.
    fn read(&self, port: &str, scratch: &Path) -> Result<String, String>;
    fn write(&self, port: &str, image: &Image, scratch: &Path) -> (bool, String);
}

/// The real thing: `avrdude` from `PATH` (`install-pi.sh` installs it), or another program in tests.
pub struct Avrdude {
    pub program: PathBuf,
    pub release: Duration,
}

impl Default for Avrdude {
    fn default() -> Self {
        Avrdude { program: "avrdude".into(), release: PORT_RELEASE }
    }
}

impl Avrdude {
    fn run(&self, args: Vec<String>) -> (bool, String) {
        std::thread::sleep(self.release);
        match Command::new(&self.program).args(&args).output() {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
                (out.status.success(), text)
            }
            Err(error) => (false, format!("{}: {error}", self.program.display())),
        }
    }
}

impl Flasher for Avrdude {
    fn read(&self, port: &str, scratch: &Path) -> Result<String, String> {
        let file = scratch.join(".read-back.hex");
        let _ = std::fs::remove_file(&file);
        let (success, output) = self.run(read_args(port, &file.to_string_lossy()));
        let read = std::fs::read_to_string(&file);
        let _ = std::fs::remove_file(&file);
        match (success, read) {
            (true, Ok(hex)) => Ok(hex),
            (true, Err(error)) => Err(format!("avrdude wrote no read-back: {error}")),
            (false, _) => Err(output),
        }
    }

    fn write(&self, port: &str, image: &Image, scratch: &Path) -> (bool, String) {
        let file = scratch.join(".flash.hex");
        if !image.intact() {
            return (false, "the image failed its SHA-256 check".into());
        }
        if let Err(error) = std::fs::write(&file, image.hex.as_bytes()) {
            return (false, format!("{}: {error}", file.display()));
        }
        let result = self.run(write_args(port, &file.to_string_lossy()));
        let _ = std::fs::remove_file(&file);
        result
    }
}

/// A new directory for one backup, named by the time it was made.
pub fn save_backup(root: &Path, files: &[(String, String)]) -> Result<String, String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
    let dir = (0..)
        .map(|n| root.join(if n == 0 { format!("{stamp}") } else { format!("{stamp}-{n}") }))
        .find(|dir| !dir.exists())
        .expect("an unused name");
    let written = std::fs::create_dir_all(&dir)
        .and_then(|_| files.iter().try_for_each(|(name, text)| std::fs::write(dir.join(name), text)));
    written.map(|_| dir.to_string_lossy().into_owned()).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Runs one job against `root` (`~/.WebControl/firmware-backups`).
pub fn run_job(root: &Path, flasher: &dyn Flasher, job: FirmwareJob) -> JobResult {
    let scratch = std::fs::create_dir_all(root).map(|_| root.to_path_buf());
    match (job, scratch) {
        (FirmwareJob::Save { files }, _) => JobResult::Saved(save_backup(root, &files)),
        (FirmwareJob::ReadFlash { .. }, Err(error)) => JobResult::Read(Err(format!("{}: {error}", root.display()))),
        (FirmwareJob::WriteFlash { .. }, Err(error)) => {
            JobResult::Written { success: false, output: format!("{}: {error}", root.display()) }
        }
        (FirmwareJob::ReadFlash { port }, Ok(dir)) => JobResult::Read(flasher.read(&port, &dir)),
        (FirmwareJob::WriteFlash { port, image }, Ok(dir)) => {
            let (success, output) = flasher.write(&port, &image, &dir);
            JobResult::Written { success, output }
        }
    }
}

const SIM_MARKER: &str = "webcontrol-rs simulated firmware";

fn hex_record(kind: u8, data: &[u8]) -> String {
    let header = [data.len() as u8, 0, 0, kind];
    let sum = header.iter().chain(data).fold(0u8, |s, b| s.wrapping_add(*b)).wrapping_neg();
    format!(":{}{sum:02X}\n", header.iter().chain(data).map(|b| format!("{b:02X}")).collect::<String>())
}

fn simulated_hex(text: &str) -> String {
    hex_record(0, text.as_bytes()) + &hex_record(1, &[])
}

/// What `--simulate` flashes when the binary carries no real image.
pub fn simulated_target() -> Image {
    Image::new(&simulated_hex(&format!("{SIM_MARKER}|51.29|7|1")))
}

/// avrdude for `--simulate`: a read-back names the running build, and a flash loads the build an image names.
pub struct SimFlasher {
    pub sim: Arc<Mutex<Sim>>,
}

impl Flasher for SimFlasher {
    fn read(&self, _port: &str, _scratch: &Path) -> Result<String, String> {
        let sim = self.sim.lock().expect("simulator lock");
        let build = sim.build();
        let rewrite = u8::from(build.rewrite_notice);
        Ok(simulated_hex(&format!("{SIM_MARKER}|{}|{}|{rewrite}", build.version, build.settings_version)))
    }

    fn write(&self, _port: &str, image: &Image, _scratch: &Path) -> (bool, String) {
        let text = application_bytes(&image.hex).ok().and_then(|bytes| String::from_utf8(bytes).ok());
        let fields: Vec<String> = text.unwrap_or_default().split('|').map(String::from).collect();
        let mut sim = self.sim.lock().expect("simulator lock");
        let banner = sim.build().banner.clone();
        let build = match fields.as_slice() {
            [marker, version, layout, rewrite] if marker == SIM_MARKER => Build {
                version: version.clone(),
                settings_version: layout.parse().unwrap_or_default(),
                rewrite_notice: rewrite == "1",
                banner,
                capabilities: None,
            },
            _ => Build::holey_51_292().on_board(&banner),
        };
        sim.flash(build);
        (true, "avrdude: flash verified (simulated)".into())
    }
}
