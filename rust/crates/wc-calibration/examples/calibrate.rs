//! Runs a calibration against the settings in `<home>/.WebControl/webcontrol.json` and prints what Accept would
//! send, to compare with Python's numbers (spec/rust-checklists/M5.md). Read-only: it never writes the file.
//! cargo run -p wc-calibration --example calibrate -- <home> holey M1 ... M12
//! cargo run -p wc-calibration --example calibrate -- <home> triangular cut12 cut34 cut5 bitDiameter

use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};
use wc_calibration::{Accept, MachineSettings, holey, triangular};
use wc_config::{Paths, Settings};

fn calculate(
    settings: &MachineSettings,
    safe_height: f64,
    kind: &str,
    values: &[Value],
) -> Result<(String, Accept), String> {
    match (kind, values) {
        ("holey", _) => holey::calibrate(settings, values).map(|r| (format!("{r:?}"), r.accept())),
        ("triangular", [cut12, cut34, cut5, bit]) => {
            let form = json!({"cut12": cut12, "cut34": cut34, "cut5": cut5, "bitDiameter": bit});
            triangular::calculate(settings, &form).map(|r| (format!("{r:?}"), r.accept(safe_height)))
        }
        _ => return Err("usage: <home> holey M1 ... M12 | <home> triangular cut12 cut34 cut5 bitDiameter".into()),
    }
    .map_err(|error| error.to_string())
}

fn run(args: &[String]) -> Result<String, String> {
    let [home, kind, numbers @ ..] = args else { return Err("usage: <home> holey|triangular <numbers>".into()) };
    let stored = Settings::load(&Paths::new(Path::new(home)));
    let lookup = |section: &str, key: &str| stored.text(section, key);
    let settings = MachineSettings::from_lookup(lookup).map_err(|error| error.to_string())?;
    let values: Vec<Value> = numbers.iter().map(|number| json!(number)).collect();
    let safe_height = lookup("Maslow Settings", "zAxisSafeHeight").and_then(|text| text.parse().ok());
    let (result, accept) = calculate(&settings, safe_height.unwrap_or(f64::NAN), kind, &values)?;
    Ok(format!("{result}\n{}", accept.queued(lookup).map_err(|error| error.to_string())?.join("\n")))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
