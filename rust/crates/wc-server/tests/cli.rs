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
    assert_eq!(parse(&args(&[]), Some(home.clone())), Ok(Cli::Serve(Options { home, port: None, simulate: false })));
}

#[test]
fn port_and_home_can_be_given() {
    let parsed = parse(&args(&["--port", "5055", "--home", "/tmp/h"]), None);
    assert_eq!(parsed, Ok(Cli::Serve(Options { home: PathBuf::from("/tmp/h"), port: Some(5055), simulate: false })));
    let equals = parse(&args(&["--port=0", "--home=/x"]), None);
    assert_eq!(equals, Ok(Cli::Serve(Options { home: PathBuf::from("/x"), port: Some(0), simulate: false })));
}

#[test]
fn bad_arguments_are_refused() {
    assert!(parse(&args(&["--port", "lots"]), Some("/h".into())).unwrap_err().contains("--port"));
    assert!(parse(&args(&["--port"]), Some("/h".into())).unwrap_err().contains("--port"));
    assert!(parse(&args(&["--simulated"]), Some("/h".into())).unwrap_err().contains("--simulated"));
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
fn the_binary_serves_on_the_port_it_reports_and_prepares_the_home_as_python_does() {
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
    let dir = home.path().join(".WebControl");
    let mut made: Vec<String> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    made.sort();
    assert_eq!(made, ["boards", "gcode", "imports", "webcontrol.json", "webcontrol.json.pre-rs"]);
    let defaults = include_str!("../../../../defaultwebcontrol.json");
    assert_eq!(std::fs::read_to_string(dir.join("webcontrol.json")).unwrap(), defaults);
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

/// `--simulate` end to end: the UI hears the simulated controller's version, and Shut Down WebControl exits 0.
#[tokio::test]
async fn the_simulated_binary_connects_and_shuts_down_from_the_ui() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let home = tempfile::tempdir().unwrap();
    let mut child = binary()
        .args(["--simulate", "--port", "0", "--home"])
        .arg(home.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let listening = lines.find_map(|l| l.ok().filter(|l| l.contains("listening on"))).expect("a listening line");
    let port = listening.rsplit(':').next().unwrap().trim().to_string();
    let url = format!("ws://127.0.0.1:{port}/ws?ns=%2FMaslowCNC");
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let version = async {
        while let Some(Ok(message)) = socket.next().await {
            // The B05 reply may come before this socket connects; every healthMessage carries the version too.
            if message.to_text().unwrap_or_default().contains("51.27") {
                return;
            }
        }
        panic!("the socket closed first");
    };
    tokio::time::timeout(Duration::from_secs(20), version).await.expect("the simulated controller's version");
    let ask = r#"{"ns":"/MaslowCNC","event":"settingRequest","data":{"data":{"section":"Advanced Settings","setting":"maxZRPM"}}}"#;
    let adopted = async {
        loop {
            socket.send(Message::text(ask)).await.unwrap();
            while let Some(Ok(message)) = socket.next().await {
                let text = message.to_text().unwrap_or_default().to_string();
                if text.contains("requestedSetting") && text.contains("maxZRPM") {
                    if text.contains("12.6") {
                        return;
                    }
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(30), adopted).await.expect("$18 adopted from the boot's $$");
    let shutdown = r#"{"ns":"/MaslowCNC","event":"action","data":{"data":{"command":"shutdown","arg":"","arg1":""}}}"#;
    socket.send(Message::text(shutdown)).await.unwrap();
    let exited = async {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let status = tokio::time::timeout(Duration::from_secs(10), exited).await.expect("exits with the socket still open");
    assert!(status.success());
    let log = std::fs::read_to_string(home.path().join(".WebControl/webcontrol-rs.log")).unwrap();
    assert!(log.contains(": Sent: B05  \n"), "{log}");
    let saved = wc_config::Settings::load(&wc_config::Paths::new(home.path()));
    assert_eq!(saved.text("Advanced Settings", "maxZRPM").as_deref(), Some("12.6"), "adopted from $$ and saved");
}
