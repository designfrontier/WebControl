use std::path::Path;
use std::process::Command;

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../frontend/dist");
    let index = dist.join("index.html");
    println!("cargo:rerun-if-changed={}", index.display());
    if !index.is_file() {
        panic!("{} is missing: run `npm ci && npm run build` in frontend/ first", index.display());
    }
    println!("cargo:rerun-if-env-changed=WC_GIT_SHA");
    let sha = std::env::var("WC_GIT_SHA").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
        Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".into())
    });
    println!("cargo:rustc-env=WC_GIT_SHA={sha}");
}
