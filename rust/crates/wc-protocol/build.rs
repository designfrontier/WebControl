use std::path::Path;

fn main() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../contract/VERSION");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let version: u32 = text.trim().parse().unwrap_or_else(|e| panic!("contract/VERSION {text:?}: {e}"));
    println!("cargo:rustc-env=WC_CONTRACT_VERSION={version}");
}
