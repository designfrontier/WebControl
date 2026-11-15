//! The upgrade's pure parts (spec/firmware.md C): board and version checks, avrdude, images and the migration.

use serde_json::json;
use wc_config::Settings;
use wc_firmware::image::{Image, PINNED_SHA256, application_bytes, rollback_image, sha256_hex};
use wc_firmware::upgrade::*;

#[test]
fn the_owners_board_and_the_other_l298_boards_may_be_upgraded() {
    for banner in ["PCB v1.1 Detected", "PCB v1.0 Detected", "PCB v1.2 Detected", "junk PCB v1.1 Detected  "] {
        let board = check_board(Some(banner)).unwrap_or_else(|e| panic!("{banner}: {e}"));
        assert!(board.drivers.is_empty());
    }
    assert_eq!(parse_board("PCB v1.1 Detected"), Some(Board { pcb: "1.1".into(), drivers: vec![] }));
}

#[test]
fn hazardous_and_unknown_boards_are_blocked_with_the_reason() {
    let blocked = |banner: Option<&str>| check_board(banner).unwrap_err();
    assert!(blocked(Some("PCB v1.6 TLE9201  Detected")).contains("wrong pins"));
    assert!(blocked(Some("PCB v1.6 TLE9201  Detected")).contains("left-encoder"));
    assert!(blocked(Some("PCB v1.3 TLE5206  Detected")).contains("hangs"));
    assert!(blocked(Some("PCB v1.4 Detected")).contains("PCB v1.4 Detected"));
    assert!(blocked(Some("PCB v1.5 TB6643  Detected")).contains("known to be safe"));
    assert!(blocked(Some("PCB v1.1 Ready")).contains("not one WebControl knows"));
    assert!(blocked(Some("PCB v2.0 Detected")).contains("not one WebControl knows"));
    assert!(blocked(None).contains("Power-cycle"));
}

#[test]
fn a_v16_tle9201_board_reads_as_a_different_board_after_51_292() {
    let before = parse_board("PCB v1.6 TLE9201  Detected");
    assert_ne!(before, parse_board("PCB v1.5 TB6643  Detected"));
    assert_eq!(parse_board("PCB v1.1 Detected"), parse_board("PCB v1.1 Detected"));
}

#[test]
fn only_holey_51_27_and_51_28_are_upgraded() {
    assert_eq!(check_version(51.27), Ok(()));
    assert_eq!(check_version(51.28), Ok(()));
    assert!(check_version(51.29).unwrap_err().contains("already"));
    assert!(check_version(0.0).unwrap_err().contains("B05"));
    assert!(check_version(1.26).unwrap_err().contains("1.26"));
    assert!(reports_target(51.29));
    assert!(!reports_target(51.27));
}

#[test]
fn avrdude_writes_the_application_with_verify_and_reads_the_flash_back() {
    assert_eq!(
        write_args("/dev/ttyACM0", "/tmp/a.hex"),
        ["-patmega2560", "-cwiring", "-P/dev/ttyACM0", "-b115200", "-D", "-Uflash:w:/tmp/a.hex:i"]
    );
    assert_eq!(read_args("/dev/ttyACM0", "/tmp/b.hex").last().unwrap(), "-Uflash:r:/tmp/b.hex:i");
    assert!(!write_args("/dev/ttyACM0", "x").iter().any(|a| a == "-V"), "-V would skip the verify");
}

#[test]
fn only_serial_device_names_reach_avrdude() {
    let good = ["/dev/ttyACM0", "/dev/ttyUSB1", "/dev/cu.usbmodem1", "COM3", "/dev/serial/by-id/usb-Arduino_0042-if00"];
    good.iter().for_each(|port| assert!(is_serial_port(port), "{port}"));
    let bad = ["/dev/ttyACM0; rm -rf ~", "$(reboot)", "-Uflash:w:evil.hex", "", "/dev/tty", "COM3x", "/dev/../etc/x"];
    bad.iter().for_each(|port| assert!(!is_serial_port(port), "{port}"));
}

#[test]
fn a_sync_failure_is_told_from_other_avrdude_failures() {
    assert_eq!(avrdude_outcome(true, "anything"), Avrdude::Done);
    assert_eq!(
        avrdude_outcome(false, "avrdude: stk500v2_getsync(): timeout communicating with programmer"),
        Avrdude::SyncFailure
    );
    assert_eq!(avrdude_outcome(false, "avrdude: stk500_recv(): programmer is not responding"), Avrdude::SyncFailure);
    assert_eq!(avrdude_outcome(false, "avrdude: ser_open(): can't open device"), Avrdude::Failed);
    assert_eq!(avrdude_outcome(false, "verification error, first mismatch at byte 0x0000"), Avrdude::Failed);
}

fn line_for(push: &wc_firmware::settings::Push, key: u32) -> Vec<&String> {
    push.lines.iter().filter(|l| l.starts_with(&format!("${key}="))).collect()
}

#[test]
fn the_migration_pushes_every_setting_with_the_stock_z_speed_and_clears_z_limits() {
    let settings = Settings::defaults();
    let push = migration_push(&settings, 51.29, None, Migration::Upgrade { preflight_z_rpm: None });
    assert_eq!(line_for(&push, 18), ["$18=12.6000"]);
    assert_eq!(line_for(&push, 17), ["$17=0.000000"], "an explicit $17");
    ["$9=", "$28=", "$36=", "$13="].iter().for_each(|k| assert!(push.lines.iter().any(|l| l.starts_with(k)), "{k}"));
    assert_eq!(&push.lines[push.lines.len() - 2..], ["$63=9999", "$64=-9999"]);
    assert!(push.adopted.contains(&("Advanced Settings".into(), "maxZRPM".into(), json!("12.6"))));
}

#[test]
fn the_migration_never_keeps_51_292s_factory_z_speed() {
    let mut settings = Settings::defaults();
    settings.set_value("Advanced Settings", "maxZRPM", json!("126"));
    let push = migration_push(&settings, 51.29, None, Migration::Upgrade { preflight_z_rpm: Some(12.6) });
    assert_eq!(line_for(&push, 18), ["$18=12.6000"]);
    let push = migration_push(&settings, 51.29, None, Migration::Upgrade { preflight_z_rpm: Some(126.0) });
    assert_eq!(line_for(&push, 18), ["$18=12.6000"]);
}

#[test]
fn the_migration_keeps_a_z_speed_the_owner_set() {
    let mut settings = Settings::defaults();
    settings.set_value("Advanced Settings", "maxZRPM", json!("15"));
    let push = migration_push(&settings, 51.29, None, Migration::Upgrade { preflight_z_rpm: Some(12.6) });
    assert_eq!(line_for(&push, 18), ["$18=15.0000"]);
    settings.set_value("Advanced Settings", "maxZRPM", json!(""));
    let push = migration_push(&settings, 51.29, None, Migration::Upgrade { preflight_z_rpm: Some(14.0) });
    assert_eq!(line_for(&push, 18), ["$18=14.0000"], "a blank takes what 51.27 reported");
}

#[test]
fn a_rollback_pushes_the_settings_without_z_limits_or_spindle_limits() {
    let settings = Settings::defaults();
    let push = migration_push(&settings, 51.27, None, Migration::Rollback);
    assert!(line_for(&push, 63).is_empty() && line_for(&push, 64).is_empty());
    assert_eq!(line_for(&push, 18), ["$18=12.6"], "the default, written as stored text like any EEPROM restore");
    assert!(push.lines.iter().all(|l| !l.starts_with("$60=") && !l.starts_with("$61=")));
}

#[test]
fn b04s_report_is_read_per_motor_and_direction() {
    let mut results = Vec::new();
    let report = [
        "Testing L motor:",
        "Direction 1 - Pass",
        "Direction 2 - Pass",
        "Testing R motor:",
        "Direction 1 - Pass",
        "Direction 2 - Pass",
        "Testing Z motor:",
        "Direction 1 - Pass",
        "Direction 2 - Fail",
        "Tests complete.",
    ];
    assert!(report.iter().all(|line| read_motor_test(&mut results, line)));
    assert!(!read_motor_test(&mut results, "ok"));
    assert!(!motor_test_passed(&results));
    results[2].passed[1] = true;
    assert!(motor_test_passed(&results));
    assert!(!read_motor_test(&mut Vec::new(), "Direction 1 - Pass"), "a verdict before any motor is not B04's");
}

fn hex_line(kind: u8, address: u16, data: &[u8]) -> String {
    let header = [data.len() as u8, (address >> 8) as u8, address as u8, kind];
    let sum = header.iter().chain(data).fold(0u8, |s, b| s.wrapping_add(*b)).wrapping_neg();
    format!(":{}{sum:02X}", header.iter().chain(data).map(|b| format!("{b:02X}")).collect::<String>())
}

#[test]
fn the_rollback_image_is_the_read_back_without_the_bootloader() {
    let read_back = [
        hex_line(0, 0x0000, &[0x0C, 0x94, 0x72, 0x00]),
        hex_line(2, 0, &[0x10, 0x00]),
        hex_line(0, 0x0000, &[0xAA, 0xFF, 0xBB]),
        hex_line(4, 0, &[0x00, 0x03]),
        hex_line(0, 0xE000, &[0x11, 0x22]),
        hex_line(5, 0, &[0, 0, 0, 0]),
        hex_line(1, 0, &[]),
    ]
    .join("\n");
    let bytes = application_bytes(&read_back).unwrap();
    assert_eq!(bytes.len(), 0x10003, "the bootloader at 0x3E000 is cut and trailing 0xFF dropped");
    assert_eq!(&bytes[..4], [0x0C, 0x94, 0x72, 0x00]);
    assert_eq!(&bytes[0x10000..], [0xAA, 0xFF, 0xBB]);
    let image = rollback_image(&read_back).unwrap();
    assert!(image.hex.contains(":020000040001F9"), "an extended linear address record past 64 KiB");
    assert_eq!(application_bytes(&image.hex).unwrap(), bytes, "the image writes the same flash");
    assert!(image.intact());
}

#[test]
fn a_damaged_read_back_never_becomes_a_rollback_image() {
    let end = hex_line(1, 0, &[]);
    let data = hex_line(0, 0, &[1, 2]);
    assert!(application_bytes(&data).unwrap_err().contains("end-of-file"));
    assert!(application_bytes(&end).unwrap_err().contains("no application"));
    assert!(application_bytes(&format!("{}\n{end}", hex_line(0, 0, &[0xFF]))).unwrap_err().contains("no application"));
    let corrupt = data.replace(":02", ":03");
    assert!(application_bytes(&format!("{corrupt}\n{end}")).unwrap_err().contains("checksum"));
    assert!(application_bytes("garbage").unwrap_err().contains("not a hex record"));
    assert!(application_bytes(":0000").unwrap_err().contains("short"));
    assert!(application_bytes(":0Z000001FF").unwrap_err().contains("bad hex"));
    assert!(application_bytes(&hex_line(6, 0, &[])).unwrap_err().contains("record type 06"));
    let long = hex_line(0, 0, &[1, 2, 3]).replacen(":03", ":02", 1);
    let fixed = format!(
        "{}{:02X}",
        &long[..long.len() - 2],
        (u8::from_str_radix(&long[long.len() - 2..], 16).unwrap()).wrapping_add(1)
    );
    assert!(application_bytes(&fixed).unwrap_err().contains("length"));
}

#[test]
fn an_image_is_pinned_by_its_sha256() {
    let image = Image::new(":00000001FF\n");
    assert_eq!(image.sha256, sha256_hex(b":00000001FF\n"));
    assert_eq!(Image::pinned(b":00000001FF\n", &image.sha256), Some(image.clone()));
    assert_eq!(Image::pinned(b":00000001FF\n", "0000"), None);
    assert_eq!(Image::pinned(b"", &sha256_hex(b"")), None);
    let tampered = Image { hex: ":00000001FE\n".into(), ..image };
    assert!(!tampered.intact());
    assert_eq!(PINNED_SHA256.trim().len(), 64, "rust/firmware/holey-51.292.sha256 holds one digest");
}

#[test]
fn a_build_without_wc_firmware_hex_carries_no_image() {
    if std::env::var_os("WC_FIRMWARE_HEX").is_none() {
        assert_eq!(Image::embedded(), None);
    } else {
        assert_eq!(Image::embedded().unwrap().sha256, PINNED_SHA256.trim());
    }
}
