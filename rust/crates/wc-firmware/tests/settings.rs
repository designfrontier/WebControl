use serde_json::{Value, json};
use wc_config::Settings;
use wc_firmware::settings::{
    Correction, Push, Sync, adopted_text, boot_push, boot_sync, corrections, firmware_key_value, parse_setting_reply,
    reinitialises_axes, setting_key, setting_line, skip_held_reinit_keys, sync_reported, values_match,
};

#[test]
fn a_boot_that_finds_every_value_held_writes_only_r() {
    let held = [(0, 2438.4), (3, 463.0), (13, 63.5), (38, 2.0), (39, 3.0)];
    let push = boot_sync(&Settings::defaults(), 51.27, &held);
    assert_eq!(push, Push { lines: vec!["$13=63.5".into()], adopted: vec![] });
}

#[test]
fn a_boot_writes_what_differs_in_key_order_with_r_once_and_adopts_unknown_keys() {
    let reported = [(38, 1.0), (13, 10.1), (3, 478.0), (18, 12.6)];
    let push = boot_sync(&Settings::defaults(), 51.27, &reported);
    assert_eq!(push.lines, ["$3=463", "$13=63.5", "$38=2"]);
    assert_eq!(push.adopted, [("Advanced Settings".into(), "maxZRPM".into(), json!("12.6"))]);
    assert_eq!(boot_sync(&Settings::defaults(), 51.27, &[]).lines, ["$13=63.5"], "nothing reported");
}

#[test]
fn only_chain_over_sprocket_reinitialises_and_a_held_one_is_never_rewritten() {
    assert!(reinitialises_axes("$38=2 ") && !reinitialises_axes("$39=3") && !reinitialises_axes("$$"));
    assert_eq!((setting_key("$RST=*"), setting_key(" $12=8113.73 ")), (None, Some(12)));
    let mut push = boot_push(&Settings::defaults(), 51.27, true);
    skip_held_reinit_keys(&mut push, &[(38, 2.0), (39, 3.0)]);
    assert!(!push.lines.iter().any(|l| l.starts_with("$38=")) && push.lines.contains(&"$39=3".to_string()));
    let mut differs = boot_push(&Settings::defaults(), 51.27, true);
    skip_held_reinit_keys(&mut differs, &[(38, 1.0)]);
    assert!(differs.lines.contains(&"$38=2".to_string()));
}

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
    assert_eq!(setting_line(13, &json!(63.5)), "$13=63.5");
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

/// Wart 4: every boot gets every setting the firmware takes, in key order, `$13` included, each as the text the
/// firmware reads as the float32 nearest the stored value (`2438.4` reads high; `2438.3998` reads exactly).
#[test]
fn a_boot_pushes_every_known_setting_in_key_order() {
    let push = boot_push(&Settings::defaults(), 51.27, false);
    assert!(push.adopted.is_empty());
    assert_eq!(
        line_values(&push.lines),
        [
            "$0=2438.3998",
            "$1=1219.1999",
            "$2=2978.3998",
            "$3=463",
            "$4=310",
            "$5=139",
            "$6=79",
            "$7=2",
            "$8=139.1",
            "$10=3360",
            "$11=1651",
            "$12=8113.7300",
            "$13=63.5",
            "$15=800",
            "$16=0",
            "$17=0",
            "$19=3.170",
            "$20=7560",
            "$21=1300",
            "$22=0",
            "$23=34",
            "$24=1",
            "$25=5",
            "$26=0",
            "$27=0.280",
            "$29=1300",
            "$30=0",
            "$31=34",
            "$32=1",
            "$33=5",
            "$34=0",
            "$35=0.280",
            "$37=33",
            "$38=2",
            "$39=3",
            "$40=0",
            "$41=0",
            "$42=2",
            "$43=0",
            "$45=0.0000051684986",
            "$46=97.9",
            // The machine sends these only to a planner build, $49 only to one with feed-forward (`link.rs`, `send_queued`).
            "$47=50",
            "$48=0.009999998",
            "$49=0",
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
    assert!(push.lines.contains(&"$17=3".to_string()));
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
        Sync::Adopt { section: "Advanced Settings".into(), key: "maxZRPM".into(), value: json!("12.6"), repair: None }
    );
    assert_eq!(sync_reported(&settings, 51.27, 18, f64::NAN), Sync::Agrees, "a NaN is never adopted");
    assert_eq!(sync_reported(&settings, 51.27, 3, 478.0), Sync::Rewrite("$3=463".into()));
    assert_eq!(sync_reported(&settings, 51.27, 3, 463.0), Sync::Agrees);
    assert_eq!(sync_reported(&settings, 51.27, 13, 10.1), Sync::Rewrite("$13=63.5".into()));
    assert_eq!(sync_reported(&settings, 51.27, 99, 1.0), Sync::Agrees, "a key WebControl does not know");
    assert_eq!(sync_reported(&settings, 51.27, 144, 1.0), Sync::Agrees, "a key the firmware rejects");
    let relay = settings_with("Advanced Settings", "spindleAutomate", json!("Relay_High"));
    assert_eq!(sync_reported(&relay, 51.27, 17, 2.0), Sync::Agrees);
    let junk = settings_with("Maslow Settings", "bedWidth", json!("wide"));
    assert_eq!(sync_reported(&junk, 51.27, 0, 1.0), Sync::Agrees, "junk text is never written");
}

#[test]
fn a_nan_pid_weight_with_no_stored_value_is_repaired_with_its_default() {
    let push = boot_sync(&Settings::defaults(), 51.27, &[(28, f64::NAN), (36, f64::NAN)]);
    assert_eq!(push.lines, ["$13=63.5", "$28=1", "$36=1"]);
    assert_eq!(
        push.adopted,
        [
            ("Advanced Settings".into(), "propWeightV".into(), json!("1")),
            ("Advanced Settings".into(), "propWeightVZ".into(), json!("1")),
        ]
    );
}

/// The owner's real webcontrol.json against the `$$` their 51.27 printed on 2026-11-14: Python wrote only `$13`.
#[test]
fn the_owners_connect_writes_only_r_on_this_and_every_later_boot() {
    let file = include_str!("fixtures/owner_webcontrol.json");
    let mut settings = Settings::from_text(file).expect("the owner's settings");
    assert!(settings.compute().is_empty(), "startup derives nothing new from the saved file");
    let transcript: Value =
        serde_json::from_str(include_str!("../../../../contract/serial/transcripts/connect_boot_handshake.json"))
            .expect("transcript");
    let events = transcript["events"].as_array().expect("events");
    let lines = events.iter().filter(|e| e["type"] == "rx").filter_map(|e| e["line"].as_str());
    let reported: Vec<(u32, f64)> = lines.filter_map(parse_setting_reply).collect();
    assert_eq!(reported.len(), 46);
    let first = boot_sync(&settings, 51.27, &reported);
    assert_eq!(first.lines, ["$13=63.5"]);
    first.adopted.iter().for_each(|(section, key, value)| {
        settings.set_value(section, key, value.clone());
    });
    assert_eq!(boot_sync(&settings, 51.27, &reported), Push { lines: vec!["$13=63.5".into()], adopted: vec![] });
}

/// The owner's `$$` after the old build wrote `$24=1.000000` and `$32=1.000000`: each reads as 1.00000012, within the
/// printed-precision tolerance of the stored 1, but above the 1 `SetTunings` accepts, so both are rewritten.
#[test]
fn a_boot_repairs_pid_values_the_firmware_refuses_even_when_they_compare_equal() {
    let settings = Settings::from_text(include_str!("fixtures/owner_webcontrol.json")).expect("the owner's settings");
    let transcript: Value =
        serde_json::from_str(include_str!("../../../../contract/serial/transcripts/connect_boot_handshake.json"))
            .expect("transcript");
    let events = transcript["events"].as_array().expect("events");
    let lines = events.iter().filter(|e| e["type"] == "rx").filter_map(|e| e["line"].as_str());
    let corrupted = |(key, value): (u32, f64)| if key == 24 || key == 32 { (key, 1.00000011) } else { (key, value) };
    let reported: Vec<(u32, f64)> = lines.filter_map(parse_setting_reply).map(corrupted).collect();
    assert!(values_match(24, &json!(1), 1.00000011), "the tolerance alone would leave it");
    assert_eq!(boot_sync(&settings, 51.27, &reported).lines, ["$13=63.5", "$24=1", "$32=1"]);
}

/// A stored weight above 1 goes out as `$24=1`, so a later boot that reads 1 back leaves it alone.
#[test]
fn a_stored_pid_value_out_of_range_is_compared_as_it_was_written() {
    let settings = settings_with("Computed Settings", "propWeightMain", json!(1.5));
    assert_eq!(sync_reported(&settings, 51.27, 24, 1.0), Sync::Agrees);
    assert_eq!(sync_reported(&settings, 51.27, 24, 0.5), Sync::Rewrite("$24=1".into()));
    let settings = settings_with("Advanced Settings", "propWeightV", json!("-2"));
    assert_eq!(sync_reported(&settings, 51.27, 28, 0.0), Sync::Agrees);
    assert_eq!(boot_sync(&settings, 51.27, &[(24, 1.0), (28, 0.0)]).lines, ["$13=63.5"]);
}

#[test]
fn a_negative_gain_is_repaired_and_an_unknown_weight_out_of_range_is_adopted_in_range_and_written() {
    let settings = Settings::defaults();
    assert_eq!(sync_reported(&settings, 51.27, 22, -0.000000001), Sync::Rewrite("$22=0".into()));
    assert_eq!(sync_reported(&settings, 51.27, 22, 0.0), Sync::Agrees);
    assert_eq!(sync_reported(&settings, 51.27, 24, f64::NAN), Sync::Rewrite("$24=1".into()));
    assert_eq!(
        sync_reported(&settings, 51.27, 28, 1.00000012),
        Sync::Adopt {
            section: "Advanced Settings".into(),
            key: "propWeightV".into(),
            value: json!("1"),
            repair: Some("$28=1".into())
        }
    );
    let push = boot_sync(&settings, 51.27, &[(36, -2.0)]);
    assert_eq!(push.lines, ["$13=63.5", "$36=0"]);
    assert_eq!(push.adopted, [("Advanced Settings".into(), "propWeightVZ".into(), json!("0"))]);
}

fn correction(key: u32, setting: &str, controller: f64, written: &str, repair: bool) -> Correction {
    Correction { key, setting: setting.into(), controller, written: written.into(), repair }
}

/// Upstream #23: what a boot's writes change, for the user; the routine `$13` changes nothing.
#[test]
fn a_boots_corrections_are_the_writes_that_change_a_reported_value() {
    let settings = Settings::defaults();
    let reported = [(3, 478.0), (13, 63.5), (24, 1.00000011), (22, -0.5), (36, -2.0), (39, 3.0)];
    let push = boot_sync(&settings, 51.27, &reported);
    assert_eq!(push.lines, ["$3=463", "$13=63.5", "$22=0", "$24=1", "$36=0"]);
    assert_eq!(
        corrections(&settings, 51.27, &push.lines, &reported),
        [
            correction(3, "motorOffsetY", 478.0, "463", false),
            correction(22, "KiPosMain", -0.5, "0", true),
            correction(24, "propWeightMain", 1.00000011, "1", true),
            correction(36, "propWeightVZ", -2.0, "0", true),
        ]
    );
    let moved_r = corrections(&settings, 51.27, &["$13=63.5".into()], &[(13, 10.1)]);
    assert_eq!(moved_r, [correction(13, "distPerRot", 10.1, "63.5", false)]);
    assert!(corrections(&settings, 51.27, &["$13=63.5".into(), "$$".into()], &[]).is_empty(), "nothing reported");
}

/// Loading takes each option's metadata from the defaults; in the owner's file that moves only descriptions and
/// titles, so no value, type, option list or `$` key changes.
#[test]
fn loading_the_owners_file_refreshes_only_descriptions_and_titles() {
    let file = include_str!("fixtures/owner_webcontrol.json");
    let stored: serde_json::Map<String, Value> = serde_json::from_str(file).expect("the owner's settings");
    let loaded = Settings::from_text(file).expect("the owner's settings");
    let meaning = |option: &serde_json::Map<String, Value>| {
        let kept = option.iter().filter(|(field, _)| !["desc", "title"].contains(&field.as_str()));
        kept.map(|(field, value)| (field.clone(), value.clone())).collect::<Vec<_>>()
    };
    let options =
        stored.iter().flat_map(|(section, list)| list.as_array().into_iter().flatten().map(move |o| (section, o)));
    let compared = options.map(|(section, option)| {
        let key = option["key"].as_str().expect("a key");
        let refreshed = loaded.option(section, key).expect("still there");
        assert_eq!(meaning(option.as_object().expect("an option")), meaning(refreshed), "{section} {key}");
    });
    assert_eq!(compared.count(), 151);
}
