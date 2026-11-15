use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use wc_config::Settings;
use wc_machine::{Core, Effect, Event, Machine, MachineHandle, Snapshot, spawn};
use wc_protocol::{Envelope, Push};
use wc_server::{AppState, PingConfig, router};
use wc_testkit::contract::Contract;

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn serve_with(machine: MachineHandle, ping: PingConfig) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(AppState::new(machine, Settings::defaults(), ping));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    address.to_string()
}

async fn serve() -> String {
    let (machine, _task) = spawn(Machine::new("10.0.0.5:5000"), None);
    serve_with(machine, PingConfig::default()).await
}

async fn open(address: &str, ns: &str) -> Client {
    let url = format!("ws://{address}/ws?ns={}", ns.replace('/', "%2F"));
    connect_async(url).await.unwrap().0
}

/// The next text frame, checked against the contract.
async fn next_frame(client: &mut Client) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), client.next()).await.unwrap().unwrap().unwrap();
        if let Message::Text(text) = message {
            Contract::shared().validate_frame(&text).unwrap();
            return serde_json::from_str(&text).unwrap();
        }
    }
}

fn decoded(frame: &Value) -> (String, Value) {
    let envelope = &frame["data"];
    let data = serde_json::from_str(envelope["data"].as_str().unwrap()).unwrap();
    (envelope["command"].as_str().unwrap().to_string(), data)
}

async fn send(client: &mut Client, frame: Value) {
    client.send(Message::text(frame.to_string())).await.unwrap();
}

fn action(command: &str, arg: &str) -> Value {
    json!({"ns": "/MaslowCNC", "event": "action", "data": {"data": {"command": command, "arg": arg, "arg1": ""}}})
}

#[tokio::test]
async fn the_first_frame_is_the_connect_handshake_with_a_fresh_sid() {
    let address = serve().await;
    let mut first = open(&address, "/MaslowCNC").await;
    let mut second = open(&address, "/MaslowCNC").await;
    let (a, b) = (next_frame(&mut first).await, next_frame(&mut second).await);
    assert_eq!(a["event"], "connect");
    assert_eq!(a["ns"], "/MaslowCNC");
    assert!(!a["data"]["sid"].as_str().unwrap().is_empty());
    assert_ne!(a["data"]["sid"], b["data"]["sid"]);
}

#[tokio::test]
async fn a_new_ui_client_hears_the_host_address_and_that_the_controller_is_disconnected() {
    let address = serve().await;
    let mut client = open(&address, "/MaslowCNC").await;
    next_frame(&mut client).await;
    let host = decoded(&next_frame(&mut client).await);
    assert_eq!(host, ("hostAddress".into(), json!({"hostAddress": "10.0.0.5:5000"})));
    let (command, status) = decoded(&next_frame(&mut client).await);
    assert_eq!((command.as_str(), status["status"].as_str()), ("connectionStatus", Some("disconnected")));
}

#[tokio::test]
async fn actions_reach_the_machine_and_its_alert_comes_back() {
    let address = serve().await;
    let mut client = open(&address, "/MaslowCNC").await;
    for _ in 0..3 {
        next_frame(&mut client).await;
    }
    send(&mut client, action("home", "")).await;
    let (command, alert) = decoded(&next_frame(&mut client).await);
    assert_eq!(command, "activateModal");
    assert_eq!(alert["message"], "Function not currently implemented.. Sorry.[home]");
}

#[tokio::test]
async fn frames_that_do_not_parse_or_belong_elsewhere_are_ignored() {
    let address = serve().await;
    let mut client = open(&address, "/MaslowCNC").await;
    for _ in 0..3 {
        next_frame(&mut client).await;
    }
    client.send(Message::text("not json")).await.unwrap();
    client.send(Message::binary(vec![1, 2, 3])).await.unwrap();
    send(&mut client, json!({"ns": "/MaslowCNCLogs", "event": "action", "data": {"data": {"command": "x"}}})).await;
    send(&mut client, json!({"ns": "/MaslowCNC", "event": "settingRequest", "data": {"data": {}}})).await;
    send(&mut client, json!({"ns": "/MaslowCNC", "event": "bogus", "data": 1})).await;
    send(&mut client, action("moveZ", "")).await;
    let (_, alert) = decoded(&next_frame(&mut client).await);
    assert_eq!(alert["message"], "Function not currently implemented.. Sorry.[moveZ]");
}

#[tokio::test]
async fn the_log_namespace_gets_its_handshake_and_no_ui_messages() {
    let address = serve().await;
    let mut logs = open(&address, "/MaslowCNCLogs").await;
    let handshake = next_frame(&mut logs).await;
    assert_eq!(handshake["ns"], "/MaslowCNCLogs");
    assert_eq!(handshake["event"], "connect");
    let mut ui = open(&address, "/MaslowCNC").await;
    for _ in 0..3 {
        next_frame(&mut ui).await;
    }
    let quiet = tokio::time::timeout(Duration::from_millis(200), logs.next()).await;
    assert!(quiet.is_err(), "the log socket got {quiet:?}");
}

/// Answers `echo <sid>` with a push meant only for that sid, and `all` with a broadcast.
struct Echo(Machine);

impl Core for Echo {
    fn handle(&mut self, event: Event, now: Duration) -> Vec<Effect> {
        match event {
            Event::Action(a) if a.command == "echo" => {
                let push = Push::message(Envelope::int("showFPSpinner", 1)).to_client(a.arg.as_str().unwrap());
                vec![Effect::Push(push)]
            }
            Event::Action(a) if a.command == "all" => {
                vec![Effect::Push(Push::message(Envelope::int("showFPSpinner", 0)))]
            }
            Event::Action(a) if a.command == "log" => {
                vec![Effect::Push(Push::log(json!({"log": "state", "state": true})))]
            }
            other => self.0.handle(other, now),
        }
    }
    fn snapshot(&self) -> Snapshot {
        self.0.snapshot()
    }
}

async fn handshake(client: &mut Client) -> String {
    let frame = next_frame(client).await;
    for _ in 0..2 {
        next_frame(client).await;
    }
    frame["data"]["sid"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn a_per_client_push_goes_only_to_its_socket() {
    let (machine, _task) = spawn(Echo(Machine::new("h")), None);
    let address = serve_with(machine, PingConfig::default()).await;
    let mut alice = open(&address, "/MaslowCNC").await;
    let alice_sid = handshake(&mut alice).await;
    let mut bob = open(&address, "/MaslowCNC").await;
    let _bob_sid = handshake(&mut bob).await;
    for _ in 0..2 {
        next_frame(&mut alice).await;
    }
    send(&mut bob, action("echo", &alice_sid)).await;
    send(&mut bob, action("all", "")).await;
    let to_alice = next_frame(&mut alice).await;
    assert_eq!(to_alice["data"]["data"], 1);
    assert_eq!(next_frame(&mut alice).await["data"]["data"], 0);
    assert_eq!(next_frame(&mut bob).await["data"]["data"], 0, "bob must not get alice's push");
}

#[tokio::test]
async fn log_pushes_reach_the_log_namespace() {
    let (machine, _task) = spawn(Echo(Machine::new("h")), None);
    let address = serve_with(machine, PingConfig::default()).await;
    let mut logs = open(&address, "/MaslowCNCLogs").await;
    next_frame(&mut logs).await;
    let mut ui = open(&address, "/MaslowCNC").await;
    handshake(&mut ui).await;
    send(&mut ui, action("log", "")).await;
    let frame = next_frame(&mut logs).await;
    assert_eq!(frame, json!({"ns": "/MaslowCNCLogs", "event": "message", "data": {"log": "state", "state": true}}));
}

fn quick_pings() -> PingConfig {
    PingConfig { interval: Duration::from_millis(100), timeout: Duration::from_millis(100) }
}

#[tokio::test]
async fn the_server_pings_and_a_client_that_answers_stays_connected() {
    let (machine, _task) = spawn(Machine::new("h"), None);
    let address = serve_with(machine, quick_pings()).await;
    let mut client = open(&address, "/MaslowCNC").await;
    handshake(&mut client).await;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(450);
    let mut pings = 0;
    while let Ok(Some(message)) = tokio::time::timeout_at(deadline, client.next()).await {
        pings += usize::from(matches!(message.unwrap(), Message::Ping(_)));
    }
    assert!(pings >= 3, "{pings} pings");
    send(&mut client, action("home", "")).await;
    let alert = loop {
        match client.next().await.unwrap().unwrap() {
            Message::Text(text) => break text,
            Message::Ping(_) => continue,
            other => panic!("{other:?}"),
        }
    };
    assert!(alert.contains("activateModal"));
}

#[tokio::test]
async fn a_client_that_never_answers_pings_is_closed() {
    let (machine, _task) = spawn(Machine::new("h"), None);
    let address = serve_with(machine, quick_pings()).await;
    let mut raw = TcpStream::connect(&address).await.unwrap();
    let upgrade = "GET /ws?ns=%2FMaslowCNC HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
        Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
    raw.write_all(upgrade.as_bytes()).await.unwrap();
    let mut received = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(3), raw.read_to_end(&mut received)).await;
    assert!(closed.is_ok(), "the server kept the silent connection open");
    assert!(String::from_utf8_lossy(&received).starts_with("HTTP/1.1 101"));
}

#[tokio::test]
async fn a_client_closing_ends_its_session() {
    let address = serve().await;
    let mut client = open(&address, "/MaslowCNC").await;
    handshake(&mut client).await;
    client.close(None).await.unwrap();
    let mut other = open(&address, "/MaslowCNC").await;
    handshake(&mut other).await;
}

/// Floods every new UI client with pushes and records each Stop it handles.
struct Flood(Machine, std::sync::Arc<tokio::sync::Notify>);

impl Core for Flood {
    fn handle(&mut self, event: Event, now: Duration) -> Vec<Effect> {
        match event {
            Event::ClientConnected { .. } => {
                let payload = vec![b'x'; 256 * 1024];
                (0..128).map(|_| Effect::Push(Push::message(Envelope::base64("gcodeUpdate", Some(&payload))))).collect()
            }
            Event::Stop(_) => {
                self.1.notify_one();
                Vec::new()
            }
            other => self.0.handle(other, now),
        }
    }
    fn snapshot(&self) -> Snapshot {
        self.0.snapshot()
    }
}

/// A client text frame with an all-zero mask, so the payload goes out as written.
fn masked_text(text: &str) -> Vec<u8> {
    let length = u16::try_from(text.len()).unwrap();
    let mut frame = vec![0x81, 0x80 | 126];
    frame.extend(length.to_be_bytes());
    frame.extend([0; 4]);
    frame.extend(text.as_bytes());
    frame
}

#[tokio::test]
async fn stop_reaches_the_machine_while_the_client_is_not_reading_its_pushes() {
    let stopped = std::sync::Arc::new(tokio::sync::Notify::new());
    let (machine, _task) = spawn(Flood(Machine::new("h"), stopped.clone()), None);
    let address = serve_with(machine, PingConfig::default()).await;
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.set_recv_buffer_size(4096).unwrap();
    let mut raw = socket.connect(address.parse().unwrap()).await.unwrap();
    let upgrade = "GET /ws?ns=%2FMaslowCNC HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
        Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
    raw.write_all(upgrade.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    raw.write_all(&masked_text(&action("stopRun", "").to_string())).await.unwrap();
    let heard = tokio::time::timeout(Duration::from_secs(3), stopped.notified()).await;
    assert!(heard.is_ok(), "Stop waited behind pushes the client had not read");
}

async fn upgrade_status(address: &str, origin: Option<&str>) -> String {
    let mut raw = TcpStream::connect(address).await.unwrap();
    let origin = origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default();
    let upgrade = format!(
        "GET /ws?ns=%2FMaslowCNC HTTP/1.1\r\nHost: {address}\r\n{origin}Upgrade: websocket\r\nConnection: Upgrade\r\n\
        Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    raw.write_all(upgrade.as_bytes()).await.unwrap();
    let mut head = [0u8; 12];
    raw.read_exact(&mut head).await.unwrap();
    String::from_utf8_lossy(&head).into_owned()
}

#[tokio::test]
async fn another_site_cannot_open_a_socket_like_socket_io_refuses_it() {
    let address = serve().await;
    assert_eq!(upgrade_status(&address, Some("http://evil.example")).await, "HTTP/1.1 403");
    assert_eq!(upgrade_status(&address, Some(&format!("http://{address}.evil.example"))).await, "HTTP/1.1 403");
    assert_eq!(upgrade_status(&address, Some(&format!("http://{address}"))).await, "HTTP/1.1 101");
    assert_eq!(upgrade_status(&address, Some(&format!("https://{address}"))).await, "HTTP/1.1 101");
    assert_eq!(upgrade_status(&address, None).await, "HTTP/1.1 101");
}
