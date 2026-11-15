use serde_json::{Value, json};
use wc_config::Settings;
use wc_firmware::settings::{
    Sync, adopted_text, boot_push, firmware_key_value, parse_setting_reply, setting_line, sync_reported, values_match,
};

/// The restore lines Python writes in `connect_eeprom_read_fail_restores_settings`, by stored value.
#[test]
fn values_are_written_the_way_config_firmware_key_value_prints_them() {
    let cases = [
        (json!("2438.4"), "2438.4"),
        (json!(0), "0.000000"),
        (json!(1300), "1300.00"),
        (json!(0.28), "0.280000"),
        (json!(1), "1.000000"),
        (json!(5), "5.00000"),
        (json!(34), "34.0000"),
        (json!(0.0000051685), "0.00000516850"),
        (json!(97.9), "97.9000"),
        (json!(63.5), "63.5000"),
        (json!(1000), "1000.000"),
        (json!(-2.5), "-2.50000"),
        (json!(true), "1.000000"),
        (json!(123456), " 123456"),
        (json!(12345678), "12345678"),
        (json!(1.0e8), "100000000.0"),
    ];
    cases.iter().for_each(|(value, text)| assert_eq!(firmware_key_value(value), *text, "{value}"));
    assert_eq!(setting_line(13, &json!(63.5)), "$13=63.5000");
}

#[test]
fn a_dollar_reply_is_read_like_config_received_setting() {
    assert_eq!(parse_setting_reply("$3=478.00000000"), Some((3, 478.0)));
    assert_eq!(parse_setting_reply("$45=  0.00000517 (chain stretch)"), Some((45, 0.00000517)));
    let nan = parse_setting_reply("$12=nan").unwrap();
    assert!(nan.0 == 12 && nan.1.is_nan());
    assert!(parse_setting_reply("$12=ovf").unwrap().1.is_nan());
    assert!(parse_setting_reply("$12=inf").unwrap().1.is_nan());
    ["$$", "$x=1", "$+1=2", "$1=", "3=4"].iter().for_each(|line| assert_eq!(parse_setting_reply(line), None, "{line}"));
}

/// Ported from test_firmware_sync: comparisons at the precision the firmware prints.
#[test]
fn replies_are_compared_at_the_printed_precision() {
    assert!(values_match(45, &json!(0.0000051685), 0.00000517), "rounded to 8 decimals");
    assert!(!values_match(45, &json!(0.0000051685), 0.00000617), "a real difference");
    assert!(values_match(10, &json!("3360.6"), 3360.0), "unsigned ints truncate");
    assert!(!values_match(10, &json!("3361"), 3360.0));
    assert!(!values_match(3, &json!("463"), f64::NAN), "a NaN reply is rewritten");
    assert!(values_match(3, &json!(""), 1.0), "an unknown value agrees with anything");
    assert!(!values_match(3, &json!("abc"), 1.0));
    assert!(values_match(16, &json!(true), 1.0));
    assert!(values_match(3, &json!("463"), 463.00000001));
}

#[test]
fn adopted_values_keep_eight_decimals_without_trailing_zeros() {
    assert_eq!(adopted_text(12.6), "12.6");
    assert_eq!(adopted_text(2000.0), "2000");
    assert_eq!(adopted_text(0.00000517), "0.00000517");
}

fn line_values(lines: &[String]) -> Vec<&str> {
    lines.iter().map(String::as_str).collect()
}

/// Wart 4: every boot gets every setting the firmware takes, in key order, `$13` included.
#[test]
fn a_boot_pushes_every_known_setting_in_key_order() {
    let push = boot_push(&Settings::defaults(), 51.27, false);
    assert!(push.adopted.is_empty());
    assert_eq!(
        line_values(&push.lines),
        [
            "$0=2438.4",
            "$1=1219.2",
            "$2=2978.4",
            "$3=463",
            "$4=310",
            "$5=139",
            "$6=79",
            "$7=2",
            "$8=139.1",
            "$10=3360",
            "$11=1651",
            "$12=8113.73",
            "$13=63.5000",
            "$15=800",
            "$16=0.000000",
            "$17=0.000000",
            "$19=3.17",
            "$20=7560.0",
            "$21=1300.00",
            "$22=0.000000",
            "$23=34.0000",
            "$24=1.000000",
            "$25=5.00000",
            "$26=0.000000",
            "$27=0.280000",
            "$29=1300.00",
            "$30=0.000000",
            "$31=34.0000",
            "$32=1.000000",
            "$33=5.00000",
            "$34=0.000000",
            "$35=0.280000",
            "$37=33",
            "$38=2.00000",
            "$39=3.00000",
            "$40=0",
            "$41=0",
            "$42=2.0",
            "$43=0",
            "$45=0.00000516850",
            "$46=97.9000",
        ]
    );
}

#[test]
fn after_an_eeprom_reset_unknown_settings_get_their_defaults() {
    let push = boot_push(&Settings::defaults(), 51.27, true);
    ["$9=2000", "$18=12.6", "$28=1", "$36=1"].iter().for_each(|line| assert!(push.lines.contains(&line.to_string())));
    let adopted: Vec<(&str, Value)> =
        push.adopted.iter().map(|(_, key, value)| (key.as_str(), value.clone())).collect();
    assert_eq!(
        adopted,
        [
            ("axisDetachTime", json!("2000")),
            ("maxZRPM", json!("12.6")),
            ("propWeightV", json!("1")),
            ("propWeightVZ", json!("1"))
        ]
    );
}

fn settings_with(section: &str, key: &str, value: Value) -> Settings {
    let mut settings = Settings::defaults();
    assert!(settings.set_value(section, key, value));
    settings
}

#[test]
fn text_that_is_not_a_number_never_goes_out() {
    let push = boot_push(&settings_with("Maslow Settings", "bedWidth", json!("wide")), 51.27, false);
    assert!(!push.lines.iter().any(|line| line.starts_with("$0=")));
}

#[test]
fn the_spindle_setting_goes_out_as_its_code() {
    let push = boot_push(&settings_with("Advanced Settings", "spindleAutomate", json!("Relay_Low")), 51.27, false);
    assert!(push.lines.contains(&"$17=3.00000".to_string()));
}

#[test]
fn optical_keys_go_only_to_custom_firmware() {
    let custom = boot_push(&Settings::defaults(), 100.0, false);
    assert!(custom.lines.contains(&"$87=0".to_string()));
    assert!(!custom.lines.iter().any(|l| l.starts_with("$85=") || l.starts_with("$86=") || l.starts_with("$144=")));
    assert!(!boot_push(&Settings::defaults(), 51.27, false).lines.contains(&"$87=0".to_string()));
}

/// Ported from test_firmware_sync: adopt the unknown, rewrite the different, leave the rest.
#[test]
fn each_reported_value_is_adopted_rewritten_or_left() {
    let settings = Settings::defaults();
    assert_eq!(
        sync_reported(&settings, 51.27, 18, 12.6),
        Sync::Adopt { section: "Advanced Settings".into(), key: "maxZRPM".into(), value: json!("12.6") }
    );
    assert_eq!(sync_reported(&settings, 51.27, 18, f64::NAN), Sync::Agrees, "a NaN is never adopted");
    assert_eq!(sync_reported(&settings, 51.27, 3, 478.0), Sync::Rewrite("$3=463".into()));
    assert_eq!(sync_reported(&settings, 51.27, 3, 463.0), Sync::Agrees);
    assert_eq!(sync_reported(&settings, 51.27, 13, 10.1), Sync::Rewrite("$13=63.5000".into()));
    assert_eq!(sync_reported(&settings, 51.27, 99, 1.0), Sync::Agrees, "a key WebControl does not know");
    assert_eq!(sync_reported(&settings, 51.27, 144, 1.0), Sync::Agrees, "a key the firmware rejects");
    let relay = settings_with("Advanced Settings", "spindleAutomate", json!("Relay_High"));
    assert_eq!(sync_reported(&relay, 51.27, 17, 2.0), Sync::Agrees);
    let junk = settings_with("Maslow Settings", "bedWidth", json!("wide"));
    assert_eq!(sync_reported(&junk, 51.27, 0, 1.0), Sync::Agrees, "junk text is never written");
}
