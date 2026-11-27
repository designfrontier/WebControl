use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use wc_machine::Host;
use wc_server::host::{LOG_FILE, LOG_LIMIT, SystemHost, busiest_core, core_times, list_ports, rotated_logs};

fn host_in(dir: &Path) -> (SystemHost, Arc<Notify>) {
    let exit = Arc::new(Notify::new());
    (SystemHost::new(&dir.join(".WebControl"), exit.clone()), exit)
}

fn names(ports: Vec<String>) -> Vec<String> {
    ports.into_iter().map(|p| p.rsplit('/').next().unwrap().to_string()).collect()
}

/// Ported from test_actions_characterization's updatePorts: Python's glob, per OS.
#[test]
fn ports_are_the_tty_devices_python_globbed() {
    let dev = tempfile::tempdir().unwrap();
    ["ttyS0", "ttyACM0", "tty1", "tty", "tty.usbmodem1", "cu.usbmodem1", "null"]
        .iter()
        .for_each(|name| std::fs::write(dev.path().join(name), "").unwrap());
    assert_eq!(names(list_ports(dev.path(), false)), ["ttyACM0", "ttyS0"]);
    assert_eq!(names(list_ports(dev.path(), true)), ["tty.usbmodem1"]);
    assert!(list_ports(&dev.path().join("missing"), false).is_empty());
    let (host, _) = host_in(dev.path());
    let host = host.with(dev.path(), &dev.path().join("stat"), &["true"]);
    assert_eq!(host.list_ports(), list_ports(dev.path(), cfg!(target_os = "macos")));
}

#[test]
fn cpu_load_is_the_busiest_core_between_samples() {
    let before = core_times("cpu  10 0 10 80 0 0 0 0 0 0\ncpu0 5 0 5 40 0 0 0 0\ncpu1 5 0 5 40 0 0 0 0\nintr 1\n");
    assert_eq!(before, [(10, 50), (10, 50)]);
    let after = core_times("cpu0 35 0 5 50 0 0 0 0\ncpu1 6 0 5 49 0 0 0 0\n");
    assert_eq!(busiest_core(&before, &after), 75.0);
    assert_eq!(busiest_core(&after, &after), 0.0);
    assert_eq!(busiest_core(&[], &after), 0.0);
}

#[test]
fn the_host_samples_proc_stat_each_time() {
    let dir = tempfile::tempdir().unwrap();
    let stat = dir.path().join("stat");
    let (host, _) = host_in(dir.path());
    let host = host.with(dir.path(), &stat, &["true"]);
    assert_eq!(host.cpu_load(), 0.0, "no file: nothing to report");
    std::fs::write(&stat, "cpu0 10 0 0 40 0 0 0 0\n").unwrap();
    assert_eq!(host.cpu_load(), 0.0, "the first sample has nothing to compare with");
    std::fs::write(&stat, "cpu0 20 0 0 50 0 0 0 0\n").unwrap();
    assert_eq!(host.cpu_load(), 50.0);
}

#[test]
fn the_serial_log_appends_rotates_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let (host, _) = host_in(dir.path());
    let path = dir.path().join(".WebControl").join(LOG_FILE);
    host.log("0.10: Sent: G21\n");
    host.log("0.20: ok\n");
    host.flush_log();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "0.10: Sent: G21\n0.20: ok\n");
    std::fs::File::options().write(true).open(&path).unwrap().set_len(LOG_LIMIT).unwrap();
    host.log("next\n");
    host.flush_log();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "next\n");
    let rotated = rotated_logs(&path);
    assert_eq!(rotated.len(), 1);
    assert_eq!(std::fs::metadata(&rotated[0]).unwrap().len(), LOG_LIMIT);
    let name = rotated[0].file_name().unwrap().to_str().unwrap().to_string();
    assert!(
        name.starts_with("webcontrol-rs.")
            && name.ends_with(".log")
            && name.len() == "webcontrol-rs.YYYYMMDD-HHMMSS.log".len()
    );
    host.clear_logs();
    host.flush_log();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    assert!(rotated_logs(&path).is_empty());
}

#[test]
fn rotation_deletes_rotated_logs_older_than_thirty_days() {
    let dir = tempfile::tempdir().unwrap();
    let (host, _) = host_in(dir.path());
    let logs = dir.path().join(".WebControl");
    let path = logs.join(LOG_FILE);
    host.log("start\n");
    host.flush_log();
    let aged = |name: &str, days: u64| {
        let file = logs.join(name);
        std::fs::write(&file, "old\n").unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 24 * 60 * 60);
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(at).unwrap();
        file
    };
    let (expired, legacy, kept) = (
        aged("webcontrol-rs.20261001-000000.log", 31),
        aged("webcontrol-rs.log.1", 40),
        aged("webcontrol-rs.20261120-000000.log", 7),
    );
    let other = aged("notes.log", 90);
    std::fs::File::options().write(true).open(&path).unwrap().set_len(LOG_LIMIT).unwrap();
    host.log("next\n");
    host.flush_log();
    assert!(!expired.exists() && !legacy.exists());
    assert!(kept.exists() && other.exists());
    assert_eq!(rotated_logs(&path).len(), 2);
}

#[test]
fn a_log_that_cannot_be_written_does_not_stop_the_host() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "").unwrap();
    let host = SystemHost::new(&blocker.join("sub"), Arc::new(Notify::new()));
    host.log("lost\n");
    host.log("lost too\n");
    host.flush_log();
    assert!(!blocker.join("sub").exists());
}

#[tokio::test]
async fn power_off_runs_its_command_and_exit_wakes_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("powered-off");
    let (host, exit) = host_in(dir.path());
    let host = host.with(dir.path(), dir.path(), &["touch", marker.to_str().unwrap()]);
    host.power_off();
    let appeared = async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), appeared).await.expect("the power-off command ran");
    host.exit();
    tokio::time::timeout(Duration::from_secs(1), exit.notified()).await.expect("exit notifies");
    let (broken, _) = host_in(dir.path());
    broken.with(dir.path(), dir.path(), &["/nonexistent/poweroff"]).power_off();
}

#[test]
fn the_default_power_off_is_the_sudoers_rule() {
    assert_eq!(wc_server::host::POWER_OFF, ["sudo", "--non-interactive", "/usr/bin/systemctl", "poweroff"]);
    let rule =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../deploy/webcontrol-rs.sudoers")).unwrap();
    assert!(rule.lines().any(|line| line == "pi ALL=(root) NOPASSWD: /usr/bin/systemctl poweroff"), "{rule}");
}

#[test]
fn create_directory_makes_one_directory_inside_the_gcode_library() {
    let dir = tempfile::tempdir().unwrap();
    let (host, _) = host_in(dir.path());
    let gcode = dir.path().join(".WebControl/gcode");
    std::fs::create_dir_all(&gcode).unwrap();
    assert!(host.create_directory("made"));
    assert!(gcode.join("made").is_dir());
    assert!(host.create_directory("made"), "an existing one is fine");
    assert!(!host.create_directory("../escaped"));
    assert!(!dir.path().join(".WebControl/escaped").exists());
    assert!(!host.create_directory("deep/er"), "like os.mkdir, no parents");
}
