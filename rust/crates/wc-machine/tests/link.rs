//! The serial link: opening, boots, flow control, Stop's resync and what controller lines lead to.

mod rig;

use rig::{Rig, alerts, commands, messages, pushed, settings, written};
use serde_json::json;
use wc_machine::{Effect, Event};
use wc_protocol::main::LinkStatus;

#[test]
fn a_new_machine_is_idle_and_disconnected() {
    let snapshot = Rig::new().machine.snapshot();
    assert_eq!((snapshot.link, snapshot.port.as_str()), (LinkStatus::Disconnected, "none"));
    assert_eq!((snapshot.upload_flag, snapshot.job_in_progress, snapshot.gcode_count), (0, false, 0));
    assert_eq!(snapshot.firmware_build, None);
    assert_eq!(snapshot.host_address, "10.0.0.5:5000");
    let state = serde_json::Value::Object(snapshot.state_fields());
    assert_eq!(state["positioningMode"], "G90");
    assert_eq!(state["units"], "MM");
    assert_eq!(state["home"], json!({"x": 0.0, "y": 0.0}));
}

#[test]
fn a_client_connecting_hears_the_host_address_then_the_link_state() {
    let mut rig = Rig::new();
    let effects = rig.send(Event::ClientConnected { sid: "s1".into() });
    let sent = messages(&effects);
    assert_eq!(sent[0], ("hostAddress".into(), json!({"hostAddress": "10.0.0.5:5000"})));
    assert_eq!(sent[1].1, json!({"status": "disconnected", "port": "none", "fakeServoStatus": false}));
    rig.open();
    rig.rx("FAKE_SERVO on");
    let effects = rig.send(Event::ClientConnected { sid: "s2".into() });
    assert_eq!(
        pushed(&effects, "connectionStatus")[0],
        json!({"status": "connected", "port": "/dev/ttyACM0",
        "fakeServoStatus": true})
    );
}

#[test]
fn a_client_that_fell_behind_hears_the_current_state_again_and_no_one_else_does() {
    let mut rig = Rig::new();
    rig.open();
    rig.machine.load_gcode("G0 X10 Y20\nG1 X30\n");
    let effects = rig.send(Event::ClientResync { sid: "s1".into() });
    let resync: Vec<_> = effects.iter().filter(|e| matches!(e, Effect::Push(_))).collect();
    assert!(resync.iter().all(|e| matches!(e, Effect::Push(p) if p.client.as_deref() == Some("s1"))));
    let sent = commands(&effects);
    let expected = ["hostAddress", "connectionStatus", "statusMessage", "requestedSetting", "homePositionMessage"];
    expected.iter().for_each(|c| assert!(sent.contains(&c.to_string()), "{c} missing from {sent:?}"));
    ["gcodePositionMessage", "gcodeUpdateCompressed", "boardDataUpdate", "boardCutDataUpdateCompressed"]
        .iter()
        .for_each(|c| assert!(sent.contains(&c.to_string()), "{c} missing from {sent:?}"));
    assert_eq!(pushed(&effects, "connectionStatus")[0]["status"], "connected");
    let settings: Vec<_> = pushed(&effects, "requestedSetting").iter().map(|d| d["setting"].clone()).collect();
    assert_eq!(settings, [json!("pauseButtonSetting"), json!("units")]);
}

#[test]
fn the_port_is_opened_from_the_settings_and_retried_every_5_s() {
    let mut idle = Rig::new();
    assert!(!idle.advance(20).iter().any(|e| matches!(e, Effect::OpenPort(_))), "no COMport, no attempt");
    let mut rig = Rig::with(settings(&[("Maslow Settings", "COMport", json!("/dev/ttyACM0"))]));
    let effects = rig.advance(10);
    assert!(effects.contains(&Effect::OpenPort("/dev/ttyACM0".into())));
    assert!(!rig.advance(100).iter().any(|e| matches!(e, Effect::OpenPort(_))), "one attempt at a time");
    rig.send(Event::PortFailed { port: "/dev/ttyACM0".into(), error: "busy".into() });
    assert!(!rig.advance(4900).iter().any(|e| matches!(e, Effect::OpenPort(_))));
    assert!(rig.advance(200).iter().any(|e| matches!(e, Effect::OpenPort(_))));
}

#[test]
fn opening_the_port_says_so_and_waits_for_the_boot() {
    let mut rig = Rig::new();
    let effects = rig.open();
    assert_eq!(pushed(&effects, "controllerMessage"), [json!("Connected on port /dev/ttyACM0")]);
    assert_eq!(pushed(&effects, "connectionStatus")[0]["status"], "connected");
    assert!(written(&rig.advance(2900)).is_empty(), "nothing goes out before the boot or 3 s");
    assert_eq!(rig.machine.snapshot().link, LinkStatus::Connected);
}

#[test]
fn an_inch_machine_boots_into_g20() {
    let mut rig = Rig::with(settings(&[("Computed Settings", "units", json!("INCHES"))]));
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|l| {
        rig.rx(l);
    });
    let sent = rig.answer_until_quiet("51.27");
    assert_eq!(sent, ["B05", "G20", "G90", "$$", "$13=63.5"]);
}

#[test]
fn a_board_that_answers_ok_without_a_banner_gets_the_handshake_at_once() {
    let mut rig = Rig::new();
    rig.open();
    assert_eq!(written(&rig.rx("ok")), ["B05"]);
}

/// Ported from test_firmware_sync: a banner within 10 s of the last one is the same boot, a later one a new boot.
#[test]
fn every_boot_asks_for_the_version_again() {
    let mut rig = Rig::connected();
    rig.rx("PCB v1.6 TLE9201  Detected");
    rig.advance(1000);
    rig.rx("Grbl v1.00");
    rig.rx("ready");
    let sent = written(&rig.rx("ok"));
    assert_eq!(sent, ["B05"]);
    rig.run_to_quiet();
    rig.advance(100);
    rig.rx("ready");
    assert_eq!(written(&rig.rx("ok")), ["B05"], "a reboot soon after the last boot is a boot all the same");
}

#[test]
fn a_boot_whose_ready_line_was_lost_still_finishes() {
    let mut rig = Rig::new();
    rig.open();
    rig.rx("PCB v1.6 TLE9201  Detected");
    rig.rx("ok");
    assert!(written(&rig.advance(1000)).is_empty(), "acks during a boot release nothing");
    assert_eq!(written(&rig.advance(2500)), ["B05"]);
}

#[test]
fn a_restart_mid_job_stops_the_job() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    rig.advance(100);
    let effects = rig.rx("PCB v1.6 TLE9201  Detected");
    assert!(alerts(&effects)[0].contains("The controller restarted"));
    assert!(!rig.machine.snapshot().job_in_progress);
}

#[test]
fn a_restart_while_paused_rewinds_to_the_lines_it_lost_and_resume_sends_them() {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(1))]));
    let file: String = (1..=40).map(|x| format!("G1 X{x}0 F100\n")).collect();
    rig.machine.load_gcode(&file);
    rig.act("startRun");
    rig.advance(300);
    rig.rx("ok");
    rig.act("pauseRun");
    assert!(rig.machine.snapshot().gcode_index > 2, "the buffer holds several lines");
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"].iter().for_each(|line| {
        rig.rx(line);
    });
    rig.run_to_quiet();
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.gcode_index), (2, 1), "G1 X20 was never acknowledged");
    rig.rx("<Idle,MPos:10.00,0.00,0.00,WPos:0.000,0.000,0.000>");
    rig.clear_written();
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert!(sent.contains(&"G1 X20 F100".to_string()), "{sent:?}");
}

#[test]
fn no_controller_line_for_5_s_closes_the_link_and_raises_an_alarm() {
    let mut rig = Rig::connected();
    let effects = rig.advance(5100);
    assert!(effects.contains(&Effect::ClosePort));
    assert_eq!(pushed(&effects, "alarm"), [json!({"message": "Alarm: Connection Failed or Invalid Firmware"})]);
    assert_eq!(pushed(&effects, "connectionStatus")[0]["status"], "disconnected");
    assert!(rig.rx("ok").is_empty(), "lines after the link closed are ignored");
}

#[test]
fn losing_the_port_mid_job_pauses_it_at_the_line_the_controller_lost() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\n");
    rig.act("startRun");
    rig.advance(100);
    let effects = rig.send(Event::PortClosed);
    assert!(messages(&effects).iter().any(|(_, d)| d["title"] == "Connection Lost"));
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.upload_flag, snapshot.job_in_progress, snapshot.gcode_index), (2, true, 0));
    assert!(rig.send(Event::PortClosed).is_empty(), "a closed port closes once");
}

#[test]
fn the_firmware_build_tells_51_292_by_its_rewrite_notice_until_the_next_boot() {
    let mut rig = Rig::connected();
    rig.rx("Firmware Version 51.29");
    assert_eq!(rig.machine.snapshot().firmware_build.as_deref(), Some("51.29x"));
    rig.rx("Change in setting, rewriting EEPROM");
    assert_eq!(rig.machine.snapshot().firmware_build.as_deref(), Some("51.292"));
    rig.rx("PCB v1.6 TLE9201  Detected");
    assert_eq!(rig.machine.snapshot().firmware_build.as_deref(), Some("51.29x"));
    assert_eq!(rig.machine.snapshot().controller_board.as_deref(), Some("PCB v1.6 TLE9201  Detected"));
}

#[test]
fn a_firmware_line_without_a_version_leaves_the_version_alone() {
    let mut rig = Rig::connected();
    let effects = rig.rx("Firmware Version unknown");
    assert_eq!(commands(&effects), ["clearAlarm", "controllerMessage"]);
    assert_eq!(rig.machine.snapshot().controller_firmware_version, 51.27);
}

#[test]
fn an_eeprom_reset_after_the_push_pushes_again() {
    let mut rig = Rig::connected();
    rig.rx("error: EEPROM read fail. Using default settings.");
    let sent = rig.run_to_quiet();
    assert_eq!(sent.first().map(String::as_str), Some("$$"), "what the reset left is read first");
    assert!(sent.contains(&"$18=12.6".to_string()));
    assert!(sent.contains(&"$38=2".to_string()), "the factory $38 differs");
    assert_eq!(sent.last().map(String::as_str), Some("$46=97.9"), "the restart replaces the read-back");
    assert_eq!(rig.machine.snapshot().settings.text("Advanced Settings", "maxZRPM").as_deref(), Some("12.6"));
    assert_eq!(rig.reboot(), ["B05", "G21", "G90", "$$", "$13=63.5"]);
}

/// Ported from test_serial_streaming: Stop jumps ahead, and nothing else goes out until a late status report.
#[test]
fn stop_writes_at_once_and_holds_commands_until_a_status_report_after_the_input_ran_dry() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("up"), json!("1"));
    let stop = rig.act("stopRun");
    assert_eq!(written(&stop), ["!"]);
    assert!(stop.iter().any(|e| matches!(e, Effect::Realtime(bytes) if bytes == b"!\n")));
    assert_eq!(commands(&stop), ["clearAlarm", "requestedSetting", "statusMessage"]);
    rig.act_with("move", json!("up"), json!("1"));
    rig.rx("ok");
    assert!(written(&rig.rx("<Idle,MPos:0,0,0>")).is_empty(), "too soon");
    rig.advance(300);
    assert_eq!(written(&rig.rx("<Idle,MPos:0,0,0>")), ["G21"]);
}

#[test]
fn stop_mid_jog_puts_the_controller_back_in_absolute_mode() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("up"), json!("1"));
    rig.rx("ok");
    rig.advance(100);
    assert_eq!(rig.machine.snapshot().state_fields()["positioningMode"], "G91", "the G00 is in flight");
    rig.act("stopRun");
    rig.clear_written();
    assert_eq!(rig.machine.snapshot().state_fields()["positioningMode"], "G90");
    rig.act_with("move", json!("up"), json!("1"));
    rig.advance(300);
    rig.rx("<Idle,MPos:0,0,0>");
    assert_eq!(rig.run_to_quiet(), ["G21", "G90", "G91", "G00 Y1.0", "G90"]);
}

#[test]
fn stop_with_a_g90_in_flight_sends_it_again_and_an_idle_stop_sends_nothing() {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(1))]));
    rig.machine.load_gcode("G91\nG0 X1\nG90\nG0 X2\n");
    rig.act("startRun");
    rig.advance(300);
    rig.act("stopRun");
    rig.clear_written();
    rig.advance(300);
    rig.rx("<Idle,MPos:0,0,0>");
    assert_eq!(rig.run_to_quiet(), ["G21", "G90"]);
    rig.act("stopRun");
    rig.clear_written();
    rig.advance(300);
    rig.rx("<Idle,MPos:0,0,0>");
    assert!(rig.run_to_quiet().is_empty());
}

#[test]
fn stop_under_an_alarm_still_restores_absolute_mode() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("up"), json!("1"));
    rig.rx("ok");
    rig.advance(100);
    rig.rx("ALARM: Position Lost");
    rig.clear_written();
    rig.act("stopRun");
    rig.advance(300);
    rig.rx("<Idle,MPos:0,0,0>");
    assert_eq!(rig.run_to_quiet(), ["!", "G21", "G90"]);
}

#[test]
fn stop_keeps_the_boots_handshake() {
    let mut rig = Rig::new();
    rig.open();
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready"].iter().for_each(|l| {
        rig.rx(l);
    });
    assert_eq!(written(&rig.act("stopRun")), ["!"]);
    rig.advance(300);
    rig.rx("<Idle,MPos:0,0,0>");
    let sent = rig.run_to_quiet();
    assert_eq!(&sent[..4], ["!", "B05", "G21", "G90"]);
}

#[test]
fn stop_while_disconnected_writes_nothing() {
    let mut rig = Rig::new();
    assert!(written(&rig.act("stopRun")).is_empty());
    assert!(written(&rig.act("stopZ")).is_empty());
}

/// Ported from test_message_processor: alarms and overflows halt and rewind to the oldest line the controller dropped.
#[test]
fn an_alarm_mid_job_halts_and_rewinds() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    rig.advance(100);
    rig.rx("ok");
    rig.advance(100);
    let effects = rig.rx("ALARM: The sled is not keeping up");
    assert_eq!(pushed(&effects, "alarm"), [json!({"message": "Alarm: Sled Not Keeping Up"})]);
    let snapshot = rig.machine.snapshot();
    assert_eq!((snapshot.job_in_progress, snapshot.upload_flag, snapshot.gcode_index), (true, 0, 1));
    assert!(written(&rig.rx("ok")).is_empty(), "acks are ignored after a halt");
    let overflow = rig.rx("Buffer overflow!");
    assert_eq!(pushed(&overflow, "alarm"), [json!({"message": "Alarm: Controller Buffer Overflow"})]);
    let lost = rig.rx("ALARM: Position Lost");
    assert_eq!(pushed(&lost, "alarm")[0]["message"], "Alarm: Position Lost.  Reset Chains.");
    assert_eq!(pushed(&rig.rx("ALARM: other"), "alarm")[0]["message"], "ALARM: other");
}

#[test]
fn a_glued_status_report_is_read_as_two_lines() {
    let mut rig = Rig::connected();
    let effects = rig.rx("<Idle,MPos:5.00,0.00,0.00,WPos:0,0,0>Motor test passed");
    assert_eq!(commands(&effects), ["positionMessage", "controllerMessage"]);
}

#[test]
fn controller_messages_become_the_uis_prompts() {
    let mut rig = Rig::connected();
    let resume = pushed(&rig.rx("Maslow Paused"), "requestedSetting");
    assert_eq!(resume, [json!({"setting": "pauseButtonSetting", "value": "Resume"})]);
    let tool = pushed(&rig.rx("Tool Change: Please insert tool 2"), "activateModal");
    assert_eq!(
        (tool[0]["message"].as_str(), tool[0]["resume"].as_str()),
        (Some("Please insert tool 2"), Some("resume"))
    );
    let adjust = pushed(&rig.rx("Message: Please adjust Z-Axis to a depth of -1.5 mm"), "activateModal");
    assert_eq!(adjust[0]["resume"], "resume");
    let unable = rig.rx("Message: Unable to find valid machine position for chain lengths 1.00, 2.00");
    assert_eq!(commands(&unable), ["alarm"]);
    let zero = rig.rx("Message: Unable to find valid machine position for chain lengths 0.00, 0.00");
    assert!(messages(&zero).is_empty());
    let other = pushed(&rig.rx("Message: Hello"), "activateModal");
    assert_eq!((other[0]["message"].as_str(), other[0]["resume"].as_str()), (Some("Hello"), Some("false")));
    assert_eq!(pushed(&rig.rx("error: Invalid statement"), "controllerMessage"), [json!("error: Invalid statement")]);
    assert_eq!(pushed(&rig.rx("Command G17 unsupported and ignored."), "controllerMessage").len(), 1);
    assert!(messages(&rig.rx("")).is_empty());
    let garbled = rig.rx("<garbled");
    assert!(messages(&garbled).is_empty());
    assert_eq!(garbled, [Effect::Log("<garbled".into())], "every line reaches the log");
}

/// Ported from test_firmware_sync: a `$$` reply adopts unknown keys and rewrites a stored value it disagrees with.
#[test]
fn a_settings_report_adopts_and_rewrites() {
    let mut rig = Rig::connected();
    assert_eq!(written(&rig.act("reportSettings")), ["$$"]);
    assert!(written(&rig.rx("$18=126.00000000")).is_empty());
    assert!(written(&rig.rx("$3=470")).is_empty(), "the rewrite waits for the $$ to be acked");
    assert!(written(&rig.advance(100)).is_empty());
    assert_eq!(written(&rig.rx("ok")), ["$3=463"]);
    assert_eq!(rig.machine.snapshot().settings.text("Advanced Settings", "maxZRPM").as_deref(), Some("126"));
}

#[test]
fn queued_commands_the_controller_cannot_take_are_refused() {
    let long = "G0 X".to_string() + &"1".repeat(130);
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "macro1", json!(long))]));
    let effects = rig.act("macro1");
    let effects = [effects, rig.advance(100)].concat();
    let modal = pushed(&effects, "activateModal");
    assert_eq!(modal[0]["title"], "Command Too Long");
    assert!(modal[0]["message"].as_str().unwrap().starts_with("That command is 1"));
    assert!(written(&effects).is_empty());
}

#[test]
fn health_reports_the_buffer_only_when_buffering_and_resends_the_status() {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(1))]));
    rig.rx("[PE:0.1,0.2,98,1650,1651]");
    let effects = rig.send(Event::Health { cpu: 12.5 });
    assert_eq!(
        pushed(&effects, "healthMessage"),
        [json!({"cpuUsage": 12.5, "bufferSize": 98,
        "controllerFirmwareVersion": 51.27})]
    );
    assert_eq!(commands(&effects), ["healthMessage", "statusMessage"]);
    let mut plain = Rig::connected();
    assert_eq!(pushed(&plain.send(Event::Health { cpu: 1.0 }), "healthMessage")[0]["bufferSize"], -1);
}

#[test]
fn ports_found_are_pushed_and_none_is_a_sentinel() {
    let mut rig = Rig::new();
    let effects = rig.send(Event::Ports(vec!["/dev/ttyACM0".into()]));
    assert_eq!(pushed(&effects, "updatePorts"), [json!(["/dev/ttyACM0"])]);
    assert_eq!(pushed(&rig.send(Event::Ports(vec![])), "updatePorts"), [json!(["None"])]);
    assert_eq!(rig.machine.snapshot().ports, ["None"]);
}

#[test]
fn writes_are_paced_50_ms_apart() {
    let mut rig = Rig::connected();
    assert_eq!(written(&rig.act_with("move", json!("up"), json!("1"))), ["G91"]);
    assert!(written(&rig.rx("ok")).is_empty(), "an ok right after a write waits out the gap");
    assert!(written(&rig.advance(40)).is_empty());
    assert_eq!(written(&rig.advance(10)), ["G00 Y1.0"]);
}

/// v0.94 slept a full 50 ms from the moment a line was due, not what was left of the 50 ms since the last write.
#[test]
fn a_line_due_10_ms_after_a_write_goes_50_ms_after_it_was_due() {
    let mut rig = Rig::connected();
    assert_eq!(written(&rig.act_with("move", json!("up"), json!("1"))), ["G91"]);
    rig.advance(10);
    assert!(written(&rig.rx("ok")).is_empty());
    assert!(written(&rig.advance(40)).is_empty(), "50 ms since the write is not enough");
    assert_eq!(written(&rig.advance(10)), ["G00 Y1.0"]);
}

#[test]
fn a_line_due_50_ms_or_more_after_a_write_goes_at_once() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("up"), json!("1"));
    rig.advance(60);
    assert_eq!(written(&rig.rx("ok")), ["G00 Y1.0"]);
    let mut job = Rig::connected();
    job.machine.load_gcode("G0 X1\nG0 X2\n");
    job.act("startRun");
    job.advance(50);
    assert_eq!(written(&job.rx("ok")), ["G0 X2"]);
}

fn realtime(effects: &[Effect]) -> Vec<&[u8]> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Realtime(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_tilde_is_never_paced_and_the_next_line_waits_50_ms_from_its_ack() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("up"), json!("1"));
    rig.advance(10);
    rig.rx("ok");
    rig.advance(10);
    rig.rx("Maslow Paused");
    let resume = rig.act("resumeRun");
    assert_eq!((written(&resume), realtime(&resume)), (vec!["~".to_string()], vec![b"~\n".as_slice()]));
    rig.advance(10);
    assert!(written(&rig.rx("ok")).is_empty());
    assert!(written(&rig.advance(40)).is_empty());
    assert_eq!(written(&rig.advance(10)), ["G00 Y1.0"]);
}

#[test]
fn stop_while_a_command_waits_out_the_pace_writes_at_once_and_drops_it() {
    let mut rig = Rig::connected();
    rig.act_with("move", json!("up"), json!("1"));
    rig.advance(10);
    let mut effects = rig.rx("ok");
    effects.extend(rig.advance(40));
    effects.extend(rig.act("stopRun"));
    assert_eq!(written(&effects), ["!"], "50 ms after the write, the line still waits");
    rig.advance(300);
    let mut effects = rig.rx("<Idle,MPos:0,0,0>");
    effects.extend(rig.advance(200));
    assert_eq!(written(&effects), ["G21"], "the jog is dropped, and its G90 restored after the units");
}

#[test]
fn stop_while_a_job_line_waits_out_the_pace_writes_at_once_and_drops_it() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\n");
    assert_eq!(written(&rig.act("startRun")), ["G0 X1"]);
    rig.advance(10);
    let mut effects = rig.rx("ok");
    effects.extend(rig.advance(40));
    effects.extend(rig.act("stopRun"));
    assert_eq!(written(&effects), ["!"]);
    rig.advance(300);
    let mut effects = rig.rx("<Idle,MPos:0,0,0>");
    effects.extend(rig.advance(200));
    assert!(written(&effects).is_empty(), "{:?}", written(&effects));
    assert_eq!(rig.machine.snapshot().upload_flag, 0);
}

#[test]
fn pause_while_a_job_line_waits_out_the_pace_holds_it_and_resume_writes_it_once() {
    let mut rig = Rig::connected();
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\n");
    rig.act("startRun");
    rig.advance(10);
    let mut effects = rig.rx("ok");
    effects.extend(rig.advance(20));
    effects.extend(rig.act("pauseRun"));
    effects.extend(rig.advance(500));
    assert!(written(&effects).is_empty(), "{:?}", written(&effects));
    rig.clear_written();
    rig.act("resumeRun");
    let sent = rig.run_to_quiet();
    assert_eq!(
        sent.iter().filter(|line| line.starts_with("G0 X")).collect::<Vec<_>>(),
        ["G0 X1.0 Y0.0", "G0 X2", "G0 X3"]
    );
}

#[test]
fn buffered_lines_fill_the_controller_in_order_each_a_full_50_ms_after_it_was_due() {
    let mut rig = Rig::connected_with(settings(&[("Maslow Settings", "bufferOn", json!(1))]));
    rig.machine.load_gcode("G0 X1\nG0 X2\nG0 X3\nG0 X4\n");
    assert_eq!(written(&rig.act("startRun")), ["G0 X1"]);
    let at: Vec<(u64, String)> =
        (1..=30).flat_map(|step| written(&rig.advance(10)).into_iter().map(move |line| (step * 10, line))).collect();
    assert_eq!(at, [(60, "G0 X2".to_string()), (120, "G0 X3".to_string()), (180, "G0 X4".to_string())]);
}
