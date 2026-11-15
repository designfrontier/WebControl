//! Runs a calibration against the settings in `<home>/.WebControl/webcontrol.json` and prints what Accept would
//! send, to compare with Python's numbers (spec/rust-checklists/M5.md). Read-only: it never writes the file.
//! cargo run -p wc-calibration --example calibrate -- <home> holey M1 ... M12
//! cargo run -p wc-calibration --example calibrate -- <home> triangular cut12 cut34 cut5 bitDiameter

use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};
use wc_calibration::{Accept, MachineSettings, holey, triangular};
use wc_config::{Paths, Settings};

fn calculate(settings: &MachineSettings, kind: &str, values: &[Value]) -> Result<(String, Accept), String> {
    match (kind, values) {
        ("holey", _) => holey::calibrate(settings, values).map(|r| (format!("{r:?}"), r.accept())),
        ("triangular", [cut12, cut34, cut5, bit]) => {
            let form = json!({"cut12": cut12, "cut34": cut34, "cut5": cut5, "bitDiameter": bit});
            triangular::calculate(settings, &form).map(|r| (format!("{r:?}"), r.accept()))
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
    let (result, accept) = calculate(&settings, kind, &values)?;
    Ok(format!("{result}\n{}", accept.queued(lookup).join("\n")))
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
