//! Track F's frozen entry point; Track F replaces this with the §3.4 recovery tests.

use wc_calibration::CalibrationError;
use wc_calibration::optical_fit::fit;
use wc_config::Settings;
use wc_protocol::optical::Cell;

#[test]
fn the_fit_is_a_stub_until_track_f() {
    let refused = fit(&Settings::defaults(), &[(Cell { i: 0, j: 0 }, (0.1, -0.2))]).unwrap_err();
    assert_eq!(refused, CalibrationError::NotImplemented("The optical geometry fit"));
    assert_eq!(refused.to_string(), "The optical geometry fit is not built yet.");
}
