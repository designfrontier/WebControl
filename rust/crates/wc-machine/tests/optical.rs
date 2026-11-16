//! Optical calibration is off until its setting is on; a Settings save stores the bool as the form sends it.

use serde_json::{Value, json};
use wc_config::Settings;
use wc_machine::optical::{self, ENABLED_KEY, ENABLED_SECTION};

fn with(value: Value) -> Settings {
    let mut settings = Settings::defaults();
    assert!(settings.store(ENABLED_SECTION, ENABLED_KEY, &value).is_some());
    settings
}

#[test]
fn the_setting_turns_optical_calibration_on() {
    let defaults = Settings::defaults();
    assert!(!optical::enabled(&defaults));
    assert!(optical::experimental(&defaults).is_empty());
    [json!(true), json!(1), json!("1")].into_iter().for_each(|on| {
        let settings = with(on.clone());
        assert!(optical::enabled(&settings), "{on}");
        assert_eq!(optical::experimental(&settings), ["optical"]);
    });
    [json!(false), json!(0)].into_iter().for_each(|off| assert!(!optical::enabled(&with(off.clone())), "{off}"));
}
