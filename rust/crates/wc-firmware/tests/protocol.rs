use wc_firmware::boot::{
    board_banner, ends_boot, firmware_build, is_boot_banner, parse_firmware_version, supports_z_limits,
};
use wc_firmware::planner::{numbered, renumber, waits_for_moves};
use wc_firmware::{
    Accounting, BUFFER_SIZE, Peaks, Planned, PositionError, Reply, RunMode, Status, classify, split_glued,
};

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

/// Line noise decodes to U+FFFD; a report ending in one must not take the machine task down with it.
#[test]
fn a_status_report_ending_in_a_multibyte_character_drops_that_character() {
    let line = "<Idle,MPos:1,2,3\u{FFFD}";
    assert_eq!(classify(line), Reply::Status(Status { mode: RunMode::Idle, x: 1.0, y: 2.0, z: 3.0 }));
    assert_eq!(classify("<Idle,MPos:1,2,3é"), classify(line));
}

/// Ported from test_ui_processor: the field count tells holey reports from stock ones.
#[test]
fn position_error_reports_are_parsed_by_their_field_count() {
    assert_eq!(
        classify("[PE:0.10,-0.20,120,1650.5,1651.25]"),
        Reply::PositionError(PositionError {
            left: 0.1,
            right: -0.2,
            space: 120,
            chains: Some((1650.5, 1651.25)),
            planner: None,
            peaks: None,
        })
    );
    assert_eq!(
        classify("[PE:0.10,-0.20,120]"),
        Reply::PositionError(PositionError {
            left: 0.1,
            right: -0.2,
            space: 120,
            chains: None,
            planner: None,
            peaks: None
        })
    );
}

/// messageProcessor's `[Measure` branch: B10's chain length, the closing bracket dropped.
#[test]
fn a_chain_measurement_is_read_from_its_bracketed_reply() {
    assert_eq!(classify("[Measure: 1651.25]"), Reply::Measured(1651.25));
    assert_eq!(classify("[Measure:-3.5]"), Reply::Measured(-3.5));
    ["[Measure: abc]", "[Measure]", "[Measure: nan]"]
        .iter()
        .for_each(|line| assert_eq!(classify(line), Reply::Other(line.to_string()), "{line}"));
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
    assert_eq!(parse_firmware_version("Firmware Version 51.30"), Some(51.3));
    assert_eq!(firmware_build(51.3, false).as_deref(), Some("51.3"), "51.30 needs no fingerprint");
    [None, Some("51.29x"), Some("51.27"), Some("100"), Some("junk")]
        .iter()
        .for_each(|build| assert!(!supports_z_limits(*build), "{build:?}"));
}

#[test]
fn accounting_counts_bytes_until_each_ack() {
    let mut accounting = Accounting::default();
    assert!(accounting.is_empty());
    accounting.sent(&"a".repeat(10), Some(4));
    accounting.sent(&"b".repeat(20), Some(5));
    assert_eq!(accounting.space(), BUFFER_SIZE - 30);
    assert_eq!(accounting.oldest_index(), Some(4));
    let tilde = accounting.realtime_sent(2);
    assert_eq!(accounting.lines(), None, "a ~ is waiting");
    assert_eq!(accounting.acked().map(|s| s.id), Some(tilde), "~ is acked ahead of the lines");
    assert_eq!(accounting.acked().map(|s| s.index), Some(Some(4)));
    assert_eq!(accounting.space(), BUFFER_SIZE - 20);
    accounting.clear();
    assert!(accounting.is_empty());
    assert_eq!(accounting.acked(), None);
    assert_eq!(accounting.oldest_index(), None);
}

#[test]
fn accounting_gives_back_the_lines_it_waits_for_oldest_first() {
    let mut accounting = Accounting::default();
    accounting.sent("G1 X1 \n", Some(1));
    accounting.sent("G1 X2 \n", Some(2));
    let lines = accounting.lines().unwrap();
    assert_eq!(lines.iter().map(|s| s.index).collect::<Vec<_>>(), [Some(1), Some(2)]);
    let taken = accounting.take();
    assert_eq!(taken.iter().map(|s| s.text.as_deref()).collect::<Vec<_>>(), [Some("G1 X1 \n"), Some("G1 X2 \n")]);
    assert!(accounting.is_empty());
    assert_eq!(accounting.lines(), Some(vec![]));
}

#[test]
fn a_planner_build_numbers_its_acks_and_resends_and_reports_its_line() {
    assert_eq!(classify("ok N12"), Reply::OkLine(12));
    assert_eq!(classify("rs N8"), Reply::Resend(8));
    assert_eq!(classify("ok Nx"), Reply::Other("ok Nx".into()));
    assert_eq!(
        classify("Capabilities: planner line-numbers"),
        Reply::Capabilities(vec!["planner".into(), "line-numbers".into()])
    );
    let Reply::PositionError(report) = classify("[PE:0.10,-0.20,127,1650.5,1651.25,42,3]") else { panic!() };
    assert_eq!(report.planner, Some(Planned { line: 42, queued: 3 }));
    assert_eq!(report.peaks, None);
}

#[test]
fn a_peak_error_build_adds_the_peaks_after_the_queue_length() {
    let Reply::PositionError(report) = classify("[PE:0.10,-0.20,127,1650.5,1651.25,42,3,0.61,0.05,0.00,0.84]") else {
        panic!()
    };
    assert_eq!(report.planner, Some(Planned { line: 42, queued: 3 }));
    let peaks = report.peaks.unwrap();
    assert_eq!(peaks, Peaks { left_ahead: 0.61, left_behind: 0.05, right_ahead: 0.0, right_behind: 0.84 });
    assert_eq!((peaks.ahead(), peaks.max()), (0.61, 0.84));
}

#[test]
fn a_numbered_line_carries_the_xor_of_its_upper_cased_trimmed_bytes() {
    assert_eq!(numbered(5, "G21"), "N5 G21*31", "4E^35^20^47^32^31 = 1F");
    assert_eq!(numbered(5, " g21  "), "N5 G21*31");
    assert_eq!(renumber(13), "M110 N13");
}

#[test]
fn an_ok_n_frees_every_copy_up_to_its_number() {
    let mut accounting = Accounting::default();
    accounting.sent("M110 N5\n", None);
    accounting.sent_numbered("G1 X1\n", 15, Some(0), 5);
    accounting.sent_numbered("G1 X2\n", 15, Some(1), 6);
    assert!(accounting.acked_through(9).is_empty(), "a late duplicate frees nothing");
    let freed: Vec<Option<u64>> = accounting.acked_through(5).iter().map(|sent| sent.number).collect();
    assert_eq!(freed, [None, Some(5)]);
    assert_eq!(accounting.space(), BUFFER_SIZE - 15);
}

#[test]
fn a_planner_build_runs_these_lines_only_after_its_queued_moves() {
    ["M5", "G0 M3", "S5000", "B08", "$47=25", "G4 P0", "g10 l20 p1", "G38.2 Z-10 F100", "G90 G4 P1"]
        .iter()
        .for_each(|line| assert!(waits_for_moves(line), "{line}"));
    ["G1 X1 F600", "G0 Z5", "G2 X1 Y1 I1 J0", "G90", "G21", "G40 G41"]
        .iter()
        .for_each(|line| assert!(!waits_for_moves(line), "{line}"));
}
