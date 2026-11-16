//! The host's half of the upgrade: avrdude (a fake script standing in), the backup directory, the simulator's
//! stand-in flasher, and the whole rehearsal through the tokio shell against `wc-sim`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::Notify;
use wc_config::Settings;
use wc_firmware::image::Image;
use wc_machine::{Event, FirmwareJob, Host, JobResult, Machine, MachineHandle, spawn};
use wc_protocol::Action;
use wc_protocol::system::UpgradeStage as Stage;
use wc_server::firmware::{Avrdude, BACKUPS, Flasher, SimFlasher, run_job, save_backup, simulated_target};
use wc_server::host::SystemHost;
use wc_server::serial::SimOpener;
use wc_sim::Build;

const READ_BACK: &str = ":0100000001FE\n:00000001FF\n";
const FAKE_AVRDUDE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
echo "$@" >> "$here/calls"
mode="$(cat "$here/mode" 2>/dev/null)"
if [ "$mode" != noread ]; then
  for arg in "$@"; do
    case "$arg" in -Uflash:r:*) file="${arg#-Uflash:r:}"; printf ':0100000001FE\n:00000001FF\n' > "${file%:i}";; esac
  done
fi
case "$mode" in
  sync) echo "avrdude: stk500v2_getsync(): timeout communicating with programmer" >&2; exit 1;;
  fail) echo "avrdude: ser_open(): can't open device" >&2; exit 1;;
esac
echo "avrdude done.  Thank you." >&2
"#;

struct Fake {
    dir: tempfile::TempDir,
}

impl Fake {
    fn new() -> Fake {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("avrdude");
        std::fs::write(&script, FAKE_AVRDUDE).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Fake { dir }
    }
    fn avrdude(&self) -> Avrdude {
        Avrdude { program: self.dir.path().join("avrdude"), release: Duration::ZERO }
    }
    fn mode(&self, mode: &str) {
        std::fs::write(self.dir.path().join("mode"), mode).unwrap();
    }
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls")).unwrap_or_default().lines().map(String::from).collect()
    }
    fn scratch(&self) -> PathBuf {
        let scratch = self.dir.path().join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        scratch
    }
}

#[test]
fn avrdude_reads_the_flash_back_into_a_scratch_file_and_cleans_up() {
    let fake = Fake::new();
    let scratch = fake.scratch();
    assert_eq!(fake.avrdude().read("/dev/ttyACM0", &scratch).unwrap(), READ_BACK);
    let file = scratch.join(".read-back.hex");
    let expected = format!("-patmega2560 -cwiring -P/dev/ttyACM0 -b115200 -D -Uflash:r:{}:i", file.display());
    assert_eq!(fake.calls(), [expected]);
    assert!(!file.exists());
    fake.mode("noread");
    assert!(fake.avrdude().read("/dev/ttyACM0", &scratch).unwrap_err().contains("wrote no read-back"));
    fake.mode("sync");
    assert!(fake.avrdude().read("/dev/ttyACM0", &scratch).unwrap_err().contains("getsync"));
}

#[test]
fn avrdude_writes_the_image_with_verify_and_reports_its_output() {
    let fake = Fake::new();
    let scratch = fake.scratch();
    let image = Image::new(READ_BACK);
    let (success, output) = fake.avrdude().write("/dev/ttyACM0", &image, &scratch);
    assert!(success && output.contains("Thank you"));
    let file = scratch.join(".flash.hex");
    assert!(fake.calls()[0].ends_with(&format!("-D -Uflash:w:{}:i", file.display())));
    assert!(!file.exists());
    fake.mode("fail");
    let (success, output) = fake.avrdude().write("/dev/ttyACM0", &image, &scratch);
    assert!(!success && output.contains("can't open device"));
    let tampered = Image { hex: ":00000001FE\n".into(), ..image.clone() };
    assert_eq!(fake.avrdude().write("p", &tampered, &scratch).1, "the image failed its SHA-256 check");
    assert!(fake.avrdude().write("p", &image, &scratch.join("missing")).1.contains("missing"));
    let absent = Avrdude { program: "/nonexistent/avrdude".into(), release: Duration::ZERO };
    assert!(absent.write("p", &image, &scratch).1.starts_with("/nonexistent/avrdude:"));
    assert_eq!(Avrdude::default().program, PathBuf::from("avrdude"));
}

#[test]
fn each_backup_gets_its_own_directory() {
    let root = tempfile::tempdir().unwrap();
    let files = vec![("settings-dump.txt".to_string(), "$0=2438.4\n".to_string())];
    let first = PathBuf::from(save_backup(root.path(), &files).unwrap());
    let second = PathBuf::from(save_backup(root.path(), &files).unwrap());
    assert_ne!(first, second);
    assert_eq!(std::fs::read_to_string(second.join("settings-dump.txt")).unwrap(), "$0=2438.4\n");
    let blocked = root.path().join("file");
    std::fs::write(&blocked, "").unwrap();
    assert!(save_backup(&blocked, &files).is_err());
}

#[test]
fn a_job_reports_a_backup_root_it_cannot_create() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("file");
    std::fs::write(&root, "").unwrap();
    let fake = Fake::new();
    let port = "/dev/ttyACM0".to_string();
    assert!(matches!(
        run_job(&root, &fake.avrdude(), FirmwareJob::ReadFlash { port: port.clone() }),
        JobResult::Read(Err(_))
    ));
    let write = FirmwareJob::WriteFlash { port, image: Image::new(READ_BACK) };
    assert!(matches!(run_job(&root, &fake.avrdude(), write), JobResult::Written { success: false, .. }));
    assert!(fake.calls().is_empty(), "avrdude never runs without its scratch directory");
}

#[test]
fn the_system_host_keeps_backups_beside_the_settings() {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join(".WebControl");
    let fake = Fake::new();
    let host = SystemHost::new(&dir, Arc::new(Notify::new())).with_flasher(Box::new(fake.avrdude()));
    let saved = host.firmware(FirmwareJob::Save { files: vec![("a.txt".into(), "x".into())] });
    let JobResult::Saved(Ok(path)) = saved else { panic!("{saved:?}") };
    assert!(Path::new(&path).starts_with(dir.join(BACKUPS)));
    let read = host.firmware(FirmwareJob::ReadFlash { port: "/dev/ttyACM0".into() });
    assert_eq!(read, JobResult::Read(Ok(READ_BACK.into())));
}

#[test]
fn the_simulators_avrdude_flashes_the_build_an_image_names() {
    let opener = SimOpener::new(Build::holey_51_27().on_board("PCB v1.1 Detected"));
    let flasher = SimFlasher { sim: opener.sim() };
    let scratch = Path::new("/nonexistent");
    let read_back = flasher.read("simulator", scratch).unwrap();
    let rollback = wc_firmware::image::rollback_image(&read_back).unwrap();
    assert!(flasher.write("simulator", &simulated_target(), scratch).0);
    assert_eq!(opener.sim().lock().unwrap().build(), &Build::holey_51_292().on_board("PCB v1.1 Detected"));
    flasher.write("simulator", &rollback, scratch);
    assert_eq!(opener.sim().lock().unwrap().build(), &Build::holey_51_27().on_board("PCB v1.1 Detected"));
    flasher.write("simulator", &Image::new(READ_BACK), scratch);
    assert_eq!(opener.sim().lock().unwrap().build().version, "51.29", "a real image is 51.292");
}

fn step(handle: &MachineHandle, step: &str) {
    handle.send(Event::Action(Action::new("upgradeHoleyFirmware", json!(step), json!(""))));
}

async fn until(handle: &MachineHandle, done: impl Fn(Stage) -> bool) -> Stage {
    for _ in 0..20_000 {
        let stage = handle.snapshot().firmware_upgrade.stage;
        if done(stage) {
            return stage;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("stuck at {:?}", handle.snapshot().firmware_upgrade);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_rehearsal_upgrades_the_simulator_and_rolls_back_through_the_shell() {
    let home = tempfile::tempdir().unwrap();
    let opener = SimOpener::new(Build::holey_51_27().on_board("PCB v1.1 Detected"));
    let sim = opener.sim();
    let host = SystemHost::new(&home.path().join(".WebControl"), Arc::new(Notify::new()))
        .with_flasher(Box::new(SimFlasher { sim: sim.clone() }));
    let machine = Machine::new("h", Settings::defaults()).on_port("simulator").with_firmware(Some(simulated_target()));
    let (handle, task) = spawn(machine, Box::new(opener), Arc::new(host));
    tokio::time::sleep(Duration::from_secs(10)).await;

    step(&handle, "check");
    assert_eq!(until(&handle, |s| s != Stage::Idle && s != Stage::Checking).await, Stage::Ready);
    step(&handle, "upgrade");
    assert_eq!(
        until(&handle, |s| matches!(s, Stage::SelfTest | Stage::Failed | Stage::RolledBack)).await,
        Stage::SelfTest
    );
    let backup = PathBuf::from(handle.snapshot().firmware_upgrade.backup.unwrap());
    assert!(backup.join("rollback.hex").is_file() && backup.join("webcontrol.json").is_file());
    assert_eq!(sim.lock().unwrap().setting(18), Some(f64::from(12.6_f32)));
    assert_eq!(handle.snapshot().firmware_build.as_deref(), Some("51.292"));

    step(&handle, "rollback");
    assert_eq!(until(&handle, |s| matches!(s, Stage::RolledBack | Stage::Failed)).await, Stage::RolledBack);
    assert_eq!(sim.lock().unwrap().build().version, "51.27");
    assert_eq!(handle.snapshot().controller_firmware_version, 51.27);
    drop(handle);
    task.await.unwrap();
}
