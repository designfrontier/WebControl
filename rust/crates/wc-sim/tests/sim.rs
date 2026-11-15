use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wc_sim::{Build, RING_BYTES, Sim, spawn};

fn sim() -> Sim {
    let mut sim = Sim::new(Build::holey_51_27());
    sim.reports_status = false;
    sim
}

fn send(sim: &mut Sim, text: &str) -> Vec<String> {
    sim.receive(text.as_bytes(), 0)
}

#[test]
fn a_boot_prints_the_banner_and_one_ok() {
    let mut sim = sim();
    assert_eq!(sim.boot(0), ["PCB v1.6 TLE9201  Detected", "FAKE_SERVO off", "Grbl v1.00", "ready", "ok"]);
    let mut reset = Sim::new(Build::holey_51_27()).with_eeprom_reset();
    assert_eq!(reset.boot(0)[2], "error: EEPROM read fail. Using default settings.");
    assert_eq!(reset.boot(0).len(), 5, "only the boot after a flash reports it");
}

#[test]
fn b05_reports_the_version_with_two_decimals() {
    assert_eq!(send(&mut sim(), "B05 \n"), ["Firmware Version 51.27", "ok"]);
    let mut newer = Sim::new(Build::holey_51_292());
    assert_eq!(newer.build().version, "51.29");
    assert_eq!(send(&mut newer, "B05\n"), ["Firmware Version 51.29", "ok"]);
}

#[test]
fn settings_are_dumped_and_written_per_build() {
    let mut sim = sim();
    let dump = send(&mut sim, "$$\n");
    assert_eq!(dump.first().map(String::as_str), Some("$0=2438.40000000"));
    assert_eq!(dump.last().map(String::as_str), Some("ok"));
    assert_eq!(send(&mut sim, "$3=470\n"), ["ok"]);
    assert_eq!(sim.setting(3), Some(470.0));
    assert_eq!(send(&mut sim, "$x=1\n"), ["error: Invalid statement", "ok"]);
    assert_eq!(send(&mut sim, "$RST=*\n"), Vec::<String>::new(), "$RST has no ok of its own");
    let mut newer = Sim::new(Build::holey_51_292());
    assert_eq!(send(&mut newer, "$18=12.6\n"), ["Change in setting, rewriting EEPROM", "ok"]);
    assert_eq!(Sim::new(Build::holey_51_292()).setting(18), Some(126.0));
}

#[test]
fn moves_follow_units_and_positioning_and_ack_when_done() {
    let mut sim = sim();
    sim.move_ms = 100;
    assert_eq!(send(&mut sim, "G91 \nG00 X10.0 Y-5 \n"), ["ok"], "G91 at once; the move is still running");
    assert_eq!(sim.tick(99), Vec::<String>::new());
    assert_eq!(sim.tick(100), ["ok"]);
    assert_eq!(sim.position(), [10.0, -5.0, 0.0]);
    sim.move_ms = 0;
    assert_eq!(send(&mut sim, "G20\nG90 G0 X1 Z0.5\n"), ["ok", "ok"]);
    assert_eq!(sim.position(), [25.4, -5.0, 12.7]);
    assert_eq!(sim.status(), "<Idle,MPos:1.00,-0.20,0.50,WPos:0.000,0.000,0.000>");
    assert_eq!(send(&mut sim, "G10 Z0\n"), ["ok"]);
    assert_eq!(sim.position()[2], 0.0);
    assert_eq!(send(&mut sim, "G17 G0 X1\n"), ["Command G17 unsupported and ignored.", "ok"]);
    assert_eq!(send(&mut sim, "B08\n"), ["ok"]);
}

#[test]
fn stop_flushes_without_acking_the_running_line() {
    let mut sim = sim();
    sim.move_ms = 1000;
    assert!(send(&mut sim, "G0 X5\nG0 X6\n").is_empty());
    assert_eq!(send(&mut sim, "!\n"), ["ok"], "the ! is acked at once and its newline dropped");
    assert_eq!(sim.tick(5000), Vec::<String>::new(), "the flushed lines never ack");
}

#[test]
fn m0_pauses_until_tilde() {
    let mut sim = sim();
    assert_eq!(send(&mut sim, "M0\n"), ["Maslow Paused"]);
    assert!(sim.is_paused());
    assert_eq!(sim.status(), "<Pause,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    assert_eq!(send(&mut sim, "G0 X1\n"), Vec::<String>::new(), "held behind the pause");
    assert_eq!(send(&mut sim, "~\n"), ["ok", "ok", "ok"], "the ~, the M0, then the move");
    assert!(!sim.is_paused());
    assert_eq!(send(&mut sim, "~"), ["ok"], "a ~ with nothing paused is acked all the same");
}

#[test]
fn a_tool_change_pauses_only_for_a_new_leading_tool() {
    let mut sim = sim();
    assert_eq!(send(&mut sim, "T0 M6\n"), ["ok"], "the same tool");
    assert_eq!(send(&mut sim, "M6 T2\n"), ["ok"], "the firmware ignores a T that does not lead");
    assert_eq!(send(&mut sim, "T2 M6\n"), ["Tool Change: Please insert tool 2"]);
    assert_eq!(send(&mut sim, "~"), ["ok", "ok"]);
    assert_eq!(send(&mut sim, "T3\n"), ["ok"], "a lone T runs as a modal move");
}

#[test]
fn m_words_are_read_as_numbers() {
    let mut sim = sim();
    assert_eq!(send(&mut sim, "M1.5\n"), ["ok"]);
    assert_eq!(send(&mut sim, "T4 M6.1\n"), ["ok"]);
    assert_eq!(send(&mut sim, "M\n"), ["ok"]);
    assert_eq!(send(&mut sim, "M01\n"), ["Maslow Paused"]);
    send(&mut sim, "~");
    assert_eq!(send(&mut sim, "T4 M06\n"), ["Tool Change: Please insert tool 4"]);
}

#[test]
fn a_probe_touches_or_misses() {
    let mut sim = sim();
    sim.receive(b"G0 Z5\n", 0);
    let touched = send(&mut sim, "G90 G38.2 Z-10 \n");
    assert_eq!(touched.last().map(String::as_str), Some("ok"));
    assert!(touched.contains(&"z axis zeroed".to_string()));
    assert_eq!(sim.position()[2], 0.0);
    sim.probe_touches = false;
    let missed = send(&mut sim, "G38.2 Z-10\n");
    assert_eq!(&missed[3..], ["error: probe did not connect", "program stopped", "z axis not set", ""]);
}

#[test]
fn an_over_long_line_overflows_and_runs_its_tail() {
    let mut sim = sim();
    let line = format!("G0 X{}\n", "1".repeat(RING_BYTES));
    let out = sim.receive(line.as_bytes(), 0);
    assert_eq!(out.first().map(String::as_str), Some("Buffer overflow!"));
    assert_eq!(out.last().map(String::as_str), Some("ok"), "the tail runs as a command and is acked");
    assert_eq!(sim.overflows(), 1);
    assert_eq!(sim.max_unread(), RING_BYTES);
}

#[test]
fn an_alarm_stops_the_firmware() {
    let mut sim = sim();
    sim.move_ms = 1000;
    sim.receive(b"G0 X5\n", 0);
    assert_eq!(sim.alarm("The sled is not keeping up"), ["ALARM: The sled is not keeping up"]);
    assert!(sim.tick(5000).is_empty());
}

#[test]
fn status_reports_come_every_200_ms() {
    let mut sim = Sim::new(Build::holey_51_27());
    sim.boot(0);
    assert!(sim.tick(199).is_empty());
    assert_eq!(sim.tick(200).len(), 1);
    assert!(sim.tick(300).is_empty());
    assert_eq!(sim.tick(400), [sim.status()]);
}

#[tokio::test(start_paused = true)]
async fn the_port_boots_then_answers() {
    let shared = Arc::new(Mutex::new(sim()));
    let mut port = spawn(shared.clone());
    let mut text = String::new();
    let mut buffer = [0u8; 512];
    while !text.ends_with("ready\r\nok\r\n") {
        let n = port.read(&mut buffer).await.unwrap();
        text.push_str(std::str::from_utf8(&buffer[..n]).unwrap());
    }
    port.write_all(b"B05\n").await.unwrap();
    let n = port.read(&mut buffer).await.unwrap();
    assert_eq!(std::str::from_utf8(&buffer[..n]).unwrap(), "Firmware Version 51.27\r\nok\r\n");
    shared.lock().unwrap().reports_status = true;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let n = port.read(&mut buffer).await.unwrap();
    assert!(std::str::from_utf8(&buffer[..n]).unwrap().starts_with("<Idle,MPos:"));
    drop(port);
}
