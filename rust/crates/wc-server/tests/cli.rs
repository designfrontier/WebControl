use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use wc_server::cli::{Command as Cli, Options, parse, version_text};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn options_default_to_the_home_directory_and_the_settings_port() {
    let home = PathBuf::from("/home/pi");
    assert_eq!(parse(&args(&[]), Some(home.clone())), Ok(Cli::Serve(Options { home, port: None })));
}

#[test]
fn port_and_home_can_be_given() {
    let parsed = parse(&args(&["--port", "5055", "--home", "/tmp/h"]), None);
    assert_eq!(parsed, Ok(Cli::Serve(Options { home: PathBuf::from("/tmp/h"), port: Some(5055) })));
    let equals = parse(&args(&["--port=0", "--home=/x"]), None);
    assert_eq!(equals, Ok(Cli::Serve(Options { home: PathBuf::from("/x"), port: Some(0) })));
}

#[test]
fn bad_arguments_are_refused() {
    assert!(parse(&args(&["--port", "lots"]), Some("/h".into())).unwrap_err().contains("--port"));
    assert!(parse(&args(&["--port"]), Some("/h".into())).unwrap_err().contains("--port"));
    assert!(parse(&args(&["--simulate"]), Some("/h".into())).unwrap_err().contains("--simulate"));
    assert!(parse(&args(&[]), None).unwrap_err().contains("HOME"));
}

#[test]
fn version_and_help_are_commands() {
    assert_eq!(parse(&args(&["--version"]), None), Ok(Cli::Version));
    assert_eq!(parse(&args(&["-h"]), None), Ok(Cli::Help));
    let text = version_text();
    assert!(text.starts_with("webcontrol-rs "), "{text}");
    assert!(text.contains(&format!("contract {}", wc_protocol::CONTRACT_VERSION)), "{text}");
}

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_webcontrol-rs"))
}

#[test]
fn the_binary_prints_its_version_and_help() {
    let out = binary().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), version_text());
    let help = binary().arg("--help").output().unwrap();
    assert!(String::from_utf8(help.stdout).unwrap().contains("--port"));
    let bad = binary().arg("--nope").output().unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8(bad.stderr).unwrap().contains("--nope"));
}

#[test]
fn the_binary_serves_on_the_port_it_reports_and_leaves_the_home_alone() {
    let home = tempfile::tempdir().unwrap();
    let mut child = binary()
        .args(["--port", "0", "--home"])
        .arg(home.path())
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let listening = lines.find_map(|l| l.ok().filter(|l| l.contains("listening on"))).expect("a listening line");
    let port = listening.rsplit(':').next().unwrap().trim().to_string();
    let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    stream.write_all(b"GET /api/state HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    let term = Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap();
    assert!(term.success());
    assert!(child.wait().unwrap().success(), "SIGTERM should shut down cleanly");
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains("\"backend\":\"rust\""), "{reply}");
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0, "the binary wrote to the home directory");
}

#[test]
fn the_binary_fails_cleanly_when_its_port_is_taken() {
    let taken = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let port = taken.local_addr().unwrap().port().to_string();
    let home = tempfile::tempdir().unwrap();
    let out = binary().args(["--port", &port, "--home"]).arg(home.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8(out.stderr).unwrap().contains("webcontrol-rs:"));
}
