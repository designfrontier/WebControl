//! Embeds the 51.292 hex named by `WC_FIRMWARE_HEX` (CI builds it with `rust/firmware/build.sh`), refusing any file
//! whose SHA-256 is not the pinned one. Without it the binary carries no image and the upgrade says so.

use std::path::Path;

use sha2::{Digest, Sha256};

fn main() {
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("holey-51.292.hex");
    let pin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../firmware/holey-51.292.sha256");
    println!("cargo:rerun-if-env-changed=WC_FIRMWARE_HEX");
    println!("cargo:rerun-if-changed={}", pin.display());
    let image = match std::env::var("WC_FIRMWARE_HEX").ok().filter(|p| !p.is_empty()) {
        None => Vec::new(),
        Some(path) => {
            println!("cargo:rerun-if-changed={path}");
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("WC_FIRMWARE_HEX {path}: {e}"));
            let pinned = std::fs::read_to_string(&pin).expect("the pinned SHA-256");
            let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            if actual != pinned.trim() {
                panic!("WC_FIRMWARE_HEX {path} has SHA-256 {actual}, but {} pins {}", pin.display(), pinned.trim());
            }
            bytes
        }
    };
    std::fs::write(&out, image).expect("write the embedded image");
}
