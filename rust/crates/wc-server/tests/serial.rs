use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use wc_config::Settings;
use wc_machine::{Machine, NoHost, PortOpener, spawn};
use wc_protocol::main::LinkStatus;
use wc_server::serial::{SerialOpener, SimOpener};
use wc_sim::Build;

/// A pseudo-terminal: the master end stands in for the controller, the slave is the serial device opened by path.
fn pty() -> (File, OwnedFd, String) {
    let (mut master, mut slave) = (0, 0);
    let opened = unsafe {
        libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
    };
    assert_eq!(opened, 0, "openpty");
    let name = unsafe { CStr::from_ptr(libc::ttyname(slave)) }.to_string_lossy().into_owned();
    unsafe { (File::from_raw_fd(master), OwnedFd::from_raw_fd(slave), name) }
}

#[tokio::test]
#[cfg_attr(target_os = "macos", ignore = "macOS ptys refuse the IOSSIOSPEED baud ioctl serialport uses")]
async fn the_serial_port_carries_lines_both_ways() {
    let (mut controller, _slave, name) = pty();
    let (reader, mut writer) = SerialOpener.open(&name).unwrap();
    writer.write_all(b"B05 \n").await.unwrap();
    writer.flush().await.unwrap();
    let mut sent = [0u8; 5];
    let mut port = controller.try_clone().unwrap();
    tokio::task::spawn_blocking(move || port.read_exact(&mut sent).map(|_| sent))
        .await
        .unwrap()
        .map(|bytes| assert_eq!(&bytes, b"B05 \n"))
        .unwrap();
    controller.write_all(b"Firmware Version 51.27\r\n").unwrap();
    let mut lines = BufReader::new(reader).lines();
    let line = tokio::time::timeout(Duration::from_secs(5), lines.next_line()).await.unwrap().unwrap();
    assert_eq!(line.as_deref(), Some("Firmware Version 51.27\r"));
}

#[tokio::test]
async fn a_missing_serial_port_fails_to_open() {
    assert!(SerialOpener.open("/dev/does-not-exist").is_err());
}

#[tokio::test]
async fn every_simulator_open_boots_the_board() {
    let mut opener = SimOpener::new(Build::holey_51_27());
    for _ in 0..2 {
        let (reader, _writer) = opener.open("simulator").unwrap();
        let mut lines = BufReader::new(reader).lines();
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("PCB v1.6 TLE9201  Detected"));
    }
    assert_eq!(opener.sim().lock().unwrap().build().version, "51.27");
}

/// `--simulate`: the machine opens the simulator, boots it and reads its firmware version.
#[tokio::test]
async fn the_machine_connects_to_the_simulator() {
    let machine = Machine::new("h", Settings::defaults()).on_port("simulator");
    let (handle, _task) = spawn(machine, Box::new(SimOpener::new(Build::holey_51_27())), Arc::new(NoHost));
    let connected = async {
        while handle.snapshot().controller_firmware_version != 51.27 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), connected).await.expect("B05 answered");
    let snapshot = handle.snapshot();
    assert_eq!((snapshot.link, snapshot.port.as_str()), (LinkStatus::Connected, "simulator"));
    assert_eq!(snapshot.controller_board.as_deref(), Some("PCB v1.6 TLE9201  Detected"));
}
