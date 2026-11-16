use serde_json::{Value, json};
use wc_config::Settings;
use wc_firmware::number::{firmware_text, in_firmware_range, read_float};
use wc_firmware::settings::{boot_push, setting_line};

/// `steps` float32 steps away from the float32 nearest `value`, away from zero.
fn ulps_above(value: f64, steps: u32) -> f32 {
    f32::from_bits((value as f32).to_bits() + steps)
}

/// Worked by hand from `readFloat`: `value` is the digits as a long, `fraction` is 0.1 multiplied in float32 once per
/// fraction digit, and the result is their float32 product.
#[test]
fn the_parser_reads_as_the_firmware_does() {
    let cases = [
        ("1.000000", ulps_above(1.0, 1)),
        ("1", 1.0),
        ("1300.00", ulps_above(1300.0, 1)),
        ("63.5000", ulps_above(63.5, 1)),
        ("63.5", 63.5),
        ("0.28", ulps_above(0.28, 1)),
        ("-0.06495", -ulps_above(0.06495, 1)),
        ("0.00000516850", ulps_above(0.0000051685, 3)),
        ("8113.73", ulps_above(8113.73, 1)),
        ("  -24", -24.0),
        ("2990.0", 2990.0),
        ("12.6", 12.6),
        (".5", 0.5),
        ("5.", 5.0),
        ("1.2.3", 1.2),
        ("12 ", 12.0),
    ];
    cases.iter().for_each(|&(text, value)| assert_eq!(read_float(text), Some(value), "{text}"));
    ["", "   ", "-", ".", "x"].iter().for_each(|text| assert_eq!(read_float(text), None, "{text}"));
}

/// Ten digits overflow the 32-bit long: 10000001192 wraps to 1410066600 before the fraction applies.
#[test]
fn the_parser_wraps_a_long_that_overflows() {
    let fraction = (0..10).fold(1.0_f32, |f, _| f * 0.1);
    assert_eq!(read_float("1.0000001192"), Some(1_410_066_600_i32 as f32 * fraction));
}

#[test]
fn pid_weights_and_gains_have_a_range_and_other_keys_do_not() {
    [24, 28, 32, 36].iter().for_each(|&key| {
        assert!(in_firmware_range(key, 1.0) && in_firmware_range(key, 0.0) && in_firmware_range(key, 0.5));
        assert!(!in_firmware_range(key, f64::from(ulps_above(1.0, 1))) && !in_firmware_range(key, -0.01));
    });
    [21, 22, 23, 25, 26, 27, 29, 30, 31, 33, 34, 35].iter().for_each(|&key| {
        assert!(in_firmware_range(key, 1300.0) && in_firmware_range(key, 0.0) && !in_firmware_range(key, -0.5));
        assert!(!in_firmware_range(key, f64::NAN));
    });
    assert!(in_firmware_range(40, -0.06495) && in_firmware_range(13, 1e30) && in_firmware_range(37, f64::NAN));
}

#[test]
fn values_are_written_as_the_shortest_text_the_firmware_reads_exactly() {
    let cases = [
        (24, 1.0, "1"),
        (21, 1300.0, "1300"),
        (13, 63.5, "63.5"),
        (27, 0.28, "0.280"),
        (40, -0.06495, "-0.064949986"),
        (2, 2990.0, "2990"),
        (19, -24.0, "-24"),
        (22, 0.0, "0"),
        (22, -0.0, "0"),
        (0, 1.0e8, "100000000"),
    ];
    cases.iter().for_each(|&(key, value, text)| assert_eq!(firmware_text(key, value, "legacy"), text, "{value}"));
}

/// Every text written reads back as the float32 nearest what was meant.
fn assert_exact(key: u32, value: f64) {
    let text = firmware_text(key, value, "legacy");
    assert_eq!(read_float(&text), Some(value as f32), "${key}={text} for {value}");
}

#[test]
fn values_whose_obvious_text_drifts_still_read_exactly() {
    [(1, 1219.2), (0, 2438.4), (12, 8113.73), (19, 3.17), (45, 0.0000051685), (41, -0.02111), (3, 433.68186)]
        .iter()
        .for_each(|&(key, value)| assert_exact(key, value));
    assert_eq!(read_float(&firmware_text(45, 0.0000051685, "legacy")), Some(0.0000051685));
}

/// A PID setting is never written outside the range `SetTunings` accepts, whatever was stored.
#[test]
fn out_of_range_pid_values_are_brought_into_range() {
    assert_eq!(firmware_text(24, 1.0000001, "legacy"), "1");
    assert_eq!(firmware_text(32, 1.5, "legacy"), "1");
    assert_eq!(firmware_text(36, -0.2, "legacy"), "0");
    assert_eq!(firmware_text(21, -3.0, "legacy"), "0");
    assert_eq!(firmware_text(40, -3.0, "legacy"), "-3", "chain tolerance may be negative");
}

/// Above 2^24 the long's conversion to float32 rounds, so digits that stay below it win among equally short texts.
#[test]
fn digits_that_convert_exactly_are_preferred() {
    assert_eq!(read_float("1677.7217"), read_float("1677.7216"));
    assert_eq!(firmware_text(0, 1677.72166, "legacy"), "1677.7216");
    assert_eq!(firmware_text(0, 2438.4, "legacy"), "2438.3998", "no text below 2^24 reads 2438.4 exactly");
}

/// A tenth digit can be what reads exactly; some values have no exact text that fits the long.
#[test]
fn every_digit_the_long_holds_is_searched() {
    assert_eq!(firmware_text(0, 14.0836, "legacy"), "14.08359827");
    assert_exact(0, 14.0836);
    let closest = firmware_text(0, 3.3565, "legacy");
    assert_ne!(read_float(&closest), Some(3.3565));
    assert_eq!(closest, "3.3565");
}

#[test]
fn a_value_no_text_reads_exactly_falls_back_to_the_closest_in_range() {
    assert_eq!(firmware_text(0, f64::NAN, "nan"), "nan");
    assert_eq!(firmware_text(0, f64::INFINITY, "inf"), "inf");
    assert_eq!(firmware_text(0, 1.0e12, "1000000000000.0"), "1000000000000.0", "beyond a 32-bit long");
    assert_eq!(firmware_text(0, 3.0e9, "3000000000.0"), "3000000000.0");
}

fn stored_numbers(settings: &Settings) -> Vec<(u32, Value)> {
    settings
        .firmware_options()
        .filter(|o| !matches!(o.firmware_key, 17 | 85 | 86 | 144))
        .filter_map(|o| wc_config::python_float(&o.value).map(|v| (o.firmware_key, json!(v))))
        .collect()
}

fn assert_lines_exact(settings: &Settings) {
    let numbers = stored_numbers(settings);
    assert!(numbers.len() > 30, "{numbers:?}");
    numbers.iter().for_each(|(key, value)| {
        let line = setting_line(*key, value);
        let text = line.split_once('=').map(|(_, text)| text).unwrap_or_default();
        let meant = value.as_f64().unwrap_or_default() as f32;
        assert_eq!(read_float(text), Some(meant), "{line} for {value}");
        assert!(in_firmware_range(*key, f64::from(meant)), "{line}");
    });
}

#[test]
fn every_owner_setting_is_written_exactly() {
    let settings = Settings::from_text(include_str!("fixtures/owner_webcontrol.json")).expect("owner settings");
    assert_lines_exact(&settings);
}

#[test]
fn every_default_setting_and_the_eeprom_restore_is_written_exactly() {
    assert_lines_exact(&Settings::defaults());
    let push = boot_push(&Settings::defaults(), 100.0, true);
    ["$13=63.5", "$24=1", "$28=1", "$32=1", "$36=1", "$27=0.280", "$45=0.0000051684986"]
        .iter()
        .for_each(|line| assert!(push.lines.contains(&line.to_string()), "{line} in {:?}", push.lines));
    push.lines.iter().for_each(|line| {
        let (key, text) = line.split_once('=').expect("a $ line");
        let read = read_float(text).expect("a number");
        assert!(in_firmware_range(key[1..].parse().unwrap(), f64::from(read)), "{line}");
    });
}
