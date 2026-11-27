//! Embeds the 51.292 hex named by `WC_FIRMWARE_HEX` and the planner 51.30 hex named by `WC_PLANNER_HEX` (CI builds
//! both with `rust/firmware/build.sh`), refusing any file whose SHA-256 is not pinned, or one without the other.

use std::path::Path;

use sha2::{Digest, Sha256};

fn embed(var: &str, version: &str) -> bool {
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join(format!("holey-{version}.hex"));
    let pin = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../firmware/holey-{version}.sha256"));
    println!("cargo:rerun-if-env-changed={var}");
    println!("cargo:rerun-if-changed={}", pin.display());
    let image = match std::env::var(var).ok().filter(|p| !p.is_empty()) {
        None => Vec::new(),
        Some(path) => {
            println!("cargo:rerun-if-changed={path}");
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{var} {path}: {e}"));
            let pinned = std::fs::read_to_string(&pin).expect("the pinned SHA-256");
            let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            if actual != pinned.trim() {
                panic!("{var} {path} has SHA-256 {actual}, but {} pins {}", pin.display(), pinned.trim());
            }
            bytes
        }
    };
    let embedded = !image.is_empty();
    std::fs::write(&out, image).expect("write the embedded image");
    embedded
}

fn main() {
    let holey = embed("WC_FIRMWARE_HEX", "51.292");
    let planner = embed("WC_PLANNER_HEX", "51.30");
    if holey != planner {
        panic!("WC_FIRMWARE_HEX and WC_PLANNER_HEX embed 51.292 and 51.30 together: set both or neither");
    }
}
