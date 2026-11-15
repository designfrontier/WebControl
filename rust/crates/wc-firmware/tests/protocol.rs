use wc_firmware::boot::{
    board_banner, ends_boot, firmware_build, is_boot_banner, parse_firmware_version, supports_z_limits,
};
use wc_firmware::line::{Positioning, positioning_set_by, problem_with, sanitise, strip_comments};
use wc_firmware::words::{changes_tool, only_selects_tool, pauses_firmware};
use wc_firmware::{Accounting, BUFFER_SIZE, PositionError, Reply, RunMode, Status, classify, split_glued};

#[test]
fn every_message_kind_is_told_apart() {
    let cases = [
        ("ok", Reply::Ok),
        ("error: probe did not connect", Reply::Error("probe did not connect".into())),
        ("$45=0.00000517 (chain stretch factor, m/m/N)", Reply::Setting { key: 45, value: 0.00000517 }),
        ("FAKE_SERVO off", Reply::FakeServo(false)),
        ("FAKE_SERVO on", Reply::FakeServo(true)),
        ("Maslow Paused", Reply::MaslowPaused),
        ("Tool Change: Please insert tool 2", Reply::ToolChange { tool: Some(2) }),
        ("Tool Change: Please insert a tool", Reply::ToolChange { tool: None }),
        ("Message: Please adjust Z-Axis", Reply::Message("Message: Please adjust Z-Axis".into())),
        ("ALARM: The sled is not keeping up", Reply::Alarm("ALARM: The sled is not keeping up".into())),
        ("Buffer overflow!", Reply::BufferOverflow),
        ("Firmware Version 51.27", Reply::FirmwareVersion(Some(51.27))),
        ("Firmware Version", Reply::FirmwareVersion(None)),
        ("$garbage", Reply::Other("$garbage".into())),
        ("Grbl v1.00", Reply::Other("Grbl v1.00".into())),
        ("<Idle,MPos:nope>", Reply::Misread("<Idle,MPos:nope>".into())),
        ("[PE:x]", Reply::Misread("[PE:x]".into())),
    ];
    cases.into_iter().for_each(|(line, reply)| assert_eq!(classify(line), reply, "{line}"));
}

#[test]
fn status_reports_carry_the_state_and_the_machine_position() {
    let status = |line: &str| match classify(line) {
        Reply::Status(status) => status,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        status("<Idle,MPos:1.50,-2.25,0.00,WPos:0.000,0.000,0.000>"),
        Status { mode: RunMode::Idle, x: 1.5, y: -2.25, z: 0.0 }
    );
    assert_eq!(status("<Pause,MPos:0,0,0,WPos:0,0,0>").mode, RunMode::Paused);
    assert_eq!(status("<Stop,MPos:0,0,0>").mode, RunMode::Stopped);
    assert_eq!(status("<Run,MPos:0,0,0>").mode, RunMode::Unknown);
    assert!(status("<Idle,MPos:nan,0,0,WPos:0,0,0>").x.is_nan());
    assert_eq!(
        [RunMode::Idle, RunMode::Paused, RunMode::Stopped, RunMode::Unknown].map(RunMode::label),
        ["Idle", "Paused", "Stopped", ""]
    );
}

/// Ported from test_ui_processor: the field count tells holey reports from stock ones.
#[test]
fn position_error_reports_are_parsed_by_their_field_count() {
    assert_eq!(
        classify("[PE:0.10,-0.20,120,1650.5,1651.25]"),
        Reply::PositionError(PositionError { left: 0.1, right: -0.2, space: 120, chains: Some((1650.5, 1651.25)) })
    );
    assert_eq!(
        classify("[PE:0.10,-0.20,120]"),
        Reply::PositionError(PositionError { left: 0.1, right: -0.2, space: 120, chains: None })
    );
}

#[test]
fn a_glued_status_line_and_a_glued_ok_are_split_off() {
    assert_eq!(split_glued("<Idle,MPos:0,0,0>Motor test passed"), ["<Idle,MPos:0,0,0>", "Motor test passed"]);
    assert_eq!(split_glued("<Idle,MPos:0,0,0><Idle,MPos:1,0,0>"), ["<Idle,MPos:0,0,0>", "<Idle,MPos:1,0,0>"]);
    assert_eq!(split_glued("the value received: 1200.00ok"), ["the value received: 1200.00", "ok"]);
    assert_eq!(split_glued("<Idle,MPos:0,0,0>ok"), ["<Idle,MPos:0,0,0>", "ok"]);
    ["ok", "Look", "<Idle,MPos:0,0,0>", "z axis zeroed"].iter().for_each(|line| assert_eq!(split_glued(line), [*line]));
}

/// Ported from test_firmware_sync: banners, glued banners, and `already` is not `ready`.
#[test]
fn boots_are_told_by_their_banners() {
    ["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "prescaler to 1PCB v1.4 Detected", "ready."]
        .iter()
        .for_each(|line| assert!(is_boot_banner(line), "{line}"));
    ["already", "readying", "ok", "Firmware Version 51.27"].iter().for_each(|l| assert!(!is_boot_banner(l), "{l}"));
    assert!(ends_boot("ready") && !ends_boot("Grbl v1.00") && !ends_boot("already"));
    assert_eq!(board_banner(" prescaler to 1PCB v1.4 Detected  ").as_deref(), Some("PCB v1.4 Detected"));
    assert_eq!(board_banner("Grbl v1.00"), None);
}

#[test]
fn the_firmware_version_is_read_from_its_b05_reply() {
    assert_eq!(parse_firmware_version("Firmware Version 51.27"), Some(51.27));
    assert_eq!(parse_firmware_version("Firmware Version   1.26 extra"), Some(1.26));
    assert_eq!(parse_firmware_version("Firmware Version51.27"), None);
    assert_eq!(parse_firmware_version("Firmware Version 51..2"), None);
    assert_eq!(parse_firmware_version("The Firmware Version 51.27"), None);
}

/// Ported from test_firmware_sync: 51.292 is told from 51.291 by its EEPROM rewrite notice.
#[test]
fn the_build_narrows_51_29_by_the_rewrite_notice() {
    assert_eq!(firmware_build(0.0, false), None);
    assert_eq!(firmware_build(51.27, true).as_deref(), Some("51.27"));
    assert_eq!(firmware_build(100.0, false).as_deref(), Some("100"));
    assert_eq!(firmware_build(51.29, false).as_deref(), Some("51.29x"));
    assert_eq!(firmware_build(51.29, true).as_deref(), Some("51.292"));
    assert!(supports_z_limits(Some("51.292")) && supports_z_limits(Some("51.3")));
    [None, Some("51.29x"), Some("51.27"), Some("100"), Some("junk")]
        .iter()
        .for_each(|build| assert!(!supports_z_limits(*build), "{build:?}"));
}

#[test]
fn accounting_counts_bytes_until_each_ack() {
    let mut accounting = Accounting::default();
    assert!(accounting.is_empty());
    accounting.sent(10, Some(4));
    accounting.sent(20, Some(5));
    assert_eq!(accounting.space(), BUFFER_SIZE - 30);
    assert_eq!(accounting.oldest_index(), Some(4));
    let tilde = accounting.realtime_sent(2);
    assert_eq!(accounting.acked(), Some(tilde), "~ is acked ahead of the lines");
    assert_eq!(accounting.acked().map(|s| s.index), Some(Some(4)));
    assert_eq!(accounting.space(), BUFFER_SIZE - 20);
    accounting.clear();
    assert!(accounting.is_empty());
    assert_eq!(accounting.acked(), None);
    assert_eq!(accounting.oldest_index(), None);
}

/// Ported from test_serial_streaming: comments and realtime characters never reach the controller.
#[test]
fn lines_are_sanitised_before_they_go_out() {
    assert_eq!(strip_comments("G0 X1 (move) Y2 ; done"), "G0 X1  Y2 ");
    assert_eq!(strip_comments("a(b(c)d)"), "ad)");
    assert_eq!(strip_comments("G0 (unclosed"), "G0 (unclosed");
    assert_eq!(sanitise("G0 X1!~`?\u{7}\u{7f} Y2 (c)"), "G0 X1 Y2 ");
}

#[test]
fn unclosed_comments_and_over_long_lines_are_refused() {
    let unclosed = problem_with("G0 (oops").unwrap();
    assert_eq!(unclosed.title, "Unclosed Comment");
    assert_eq!(problem_with(&"X".repeat(125)), None, "126 bytes with the newline fit");
    let long = problem_with(&"X".repeat(126)).unwrap();
    assert_eq!(
        (long.title, long.reason.as_str()),
        ("Too Long", "is 127 characters; the controller accepts at most 126.")
    );
    assert!(long.advice.contains("Truncate"));
}

#[test]
fn the_positioning_mode_follows_the_last_g90_or_g91() {
    assert_eq!(positioning_set_by("G91 G0 X1"), Some(Positioning::Relative));
    assert_eq!(positioning_set_by("G91 G90"), Some(Positioning::Absolute));
    assert_eq!(positioning_set_by("G0 X1"), None);
    assert_eq!([Positioning::Absolute, Positioning::Relative].map(|p| (p.code(), p.word())), [(0, "G90"), (1, "G91")]);
}

/// Ported from test_program_pause / test_tool_change: which words pause the firmware.
#[test]
fn m0_m1_and_m6_are_found_however_they_are_padded() {
    ["M0", "M1", "M00", "M01", "G0 X1 M0", "M000"].iter().for_each(|l| assert!(pauses_firmware(l), "{l}"));
    ["M10", "M100", "M3", "M1.5", "M", "G0 X1"].iter().for_each(|l| assert!(!pauses_firmware(l), "{l}"));
    ["M6", "M06", "T2 M6", "M6 T2"].iter().for_each(|l| assert!(changes_tool(l), "{l}"));
    ["M60", "M16", "M6.1", "T2"].iter().for_each(|l| assert!(!changes_tool(l), "{l}"));
}

#[test]
fn a_lone_t_only_selects_a_tool() {
    ["T2", " T2 ", "T 2", "N10 T2", "N10T2.", "T2.5 "].iter().for_each(|l| assert!(only_selects_tool(l), "{l}"));
    ["T", "T2 M6", "G0 T2", "N T2", "NT2", "T2 X1", "2"].iter().for_each(|l| assert!(!only_selects_tool(l), "{l}"));
}
