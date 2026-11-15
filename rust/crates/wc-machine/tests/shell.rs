//! The tokio shell: the machine task's channels, the port it owns and the host it reaches.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines};
use tokio::sync::mpsc;
use wc_config::Settings;
use wc_machine::{Core, Effect, Event, Host, Machine, NoHost, NoPort, PortOpener, Serial, Snapshot, spawn};
use wc_protocol::main::LinkStatus;
use wc_protocol::{Action, Envelope, Namespace, Push};

fn action(command: &str) -> Action {
    Action { command: command.into(), arg: json!(""), arg1: json!("") }
}

fn with_port() -> Machine {
    let mut settings = Settings::defaults();
    settings.set_value("Maslow Settings", "COMport", json!("/dev/ttyACM0"));
    Machine::new("h", settings)
}

/// Hands the test the controller's end of every port the shell opens.
struct Wire {
    ends: mpsc::UnboundedSender<DuplexStream>,
    opened: Arc<Mutex<Vec<String>>>,
}

impl PortOpener for Wire {
    fn open(&mut self, port: &str) -> std::io::Result<Serial> {
        self.opened.lock().unwrap().push(port.into());
        let (ours, theirs) = tokio::io::duplex(4096);
        self.ends.send(theirs).unwrap();
        let (reader, writer) = tokio::io::split(ours);
        Ok((Box::new(reader), Box::new(writer)))
    }
}

fn wire() -> (Box<Wire>, mpsc::UnboundedReceiver<DuplexStream>, Arc<Mutex<Vec<String>>>) {
    let (ends, receiver) = mpsc::unbounded_channel();
    let opened = Arc::new(Mutex::new(Vec::new()));
    (Box::new(Wire { ends, opened: opened.clone() }), receiver, opened)
}

#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<String>>,
}

impl Recorder {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn note(&self, call: &str) {
        self.calls.lock().unwrap().push(call.into());
    }
}

impl Host for Recorder {
    fn list_ports(&self) -> Vec<String> {
        self.note("list_ports");
        vec!["/dev/ttyACM0".into()]
    }
    fn log(&self, line: &str) {
        self.note(&format!("log {}", line.split_once(": ").map_or(line, |(_, rest)| rest).trim_end()));
    }
    fn clear_logs(&self) {
        self.note("clear_logs");
    }
    fn power_off(&self) {
        self.note("power_off");
    }
    fn exit(&self) {
        self.note("exit");
    }
    fn cpu_load(&self) -> f64 {
        12.5
    }
}

struct Controller {
    lines: Lines<BufReader<tokio::io::ReadHalf<DuplexStream>>>,
    writer: tokio::io::WriteHalf<DuplexStream>,
}

impl Controller {
    fn new(end: DuplexStream) -> Controller {
        let (reader, writer) = tokio::io::split(end);
        Controller { lines: BufReader::new(reader).lines(), writer }
    }
    async fn say(&mut self, lines: &[&str]) {
        let text: String = lines.iter().map(|l| format!("{l}\r\n")).collect();
        self.writer.write_all(text.as_bytes()).await.unwrap();
    }
    async fn next(&mut self) -> String {
        tokio::time::timeout(Duration::from_secs(30), self.lines.next_line()).await.unwrap().unwrap().unwrap()
    }
}

/// Records what it handled; answers every action with a write and a push naming it, and opens on `open`.
struct Echo {
    seen: Arc<Mutex<Vec<String>>>,
    inner: Machine,
}

impl Core for Echo {
    fn handle(&mut self, event: Event, _: Duration) -> Vec<Effect> {
        let (label, write) = match &event {
            Event::Action(a) if a.command == "open" => return vec![Effect::OpenPort("p".into())],
            Event::Action(a) => (a.command.clone(), Effect::Write(format!("{}\n", a.command).into_bytes())),
            Event::Stop(a) => (a.command.clone(), Effect::Realtime(b"!\n".to_vec())),
            _ => return Vec::new(),
        };
        self.seen.lock().unwrap().push(label);
        vec![write, Effect::Push(Push::message(Envelope::int("showFPSpinner", 1)))]
    }
    fn snapshot(&self) -> Snapshot {
        self.inner.snapshot()
    }
}

async fn stop_first(round: usize) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (opener, mut ends, _) = wire();
    let (handle, task) = spawn(Echo { seen: seen.clone(), inner: with_port() }, opener, Arc::new(NoHost));
    handle.send(Event::Action(action("open")));
    let mut controller = Controller::new(ends.recv().await.unwrap());
    ["move", "moveZ", "home"].into_iter().for_each(|c| handle.send(Event::Action(action(c))));
    handle.send(Event::Action(action("stopRun")));
    let written = [controller.next().await, controller.next().await, controller.next().await, controller.next().await];
    assert_eq!(written, ["!", "move", "moveZ", "home"], "round {round}");
    assert_eq!(*seen.lock().unwrap(), ["stopRun", "move", "moveZ", "home"]);
    drop(handle);
    task.await.unwrap();
}

/// Repeated because an unbiased select would still put Stop first now and then.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stop_jumps_the_queue_of_commands() {
    for round in 0..16 {
        stop_first(round).await;
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_configured_port_is_opened_and_carries_the_handshake() {
    let (opener, mut ends, opened) = wire();
    let host = Arc::new(Recorder::default());
    let (handle, _task) = spawn(with_port(), opener, host.clone());
    let mut pushes = handle.subscribe();
    let mut controller = Controller::new(ends.recv().await.unwrap());
    assert_eq!(*opened.lock().unwrap(), ["/dev/ttyACM0"]);
    controller.say(&["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"]).await;
    assert_eq!(controller.next().await, "B05  ");
    controller.say(&["Firmware Version 51.27", "ok"]).await;
    assert_eq!(controller.next().await, "G20  ", "the default units are inches");
    assert_eq!(handle.snapshot().controller_firmware_version, 51.27);
    assert_eq!(handle.snapshot().link, LinkStatus::Connected);
    assert!(host.calls().contains(&"log Sent: B05".to_string()), "{:?}", host.calls());
    assert!(host.calls().contains(&"log Firmware Version 51.27".to_string()));
    let logs: Vec<Value> = std::iter::from_fn(|| pushes.try_recv().ok())
        .filter(|push| push.ns == Namespace::Logs)
        .map(|push| push.data)
        .collect();
    assert!(logs.iter().any(|log| log["log"] == "alog" && log["data"].as_str().unwrap().ends_with(": Sent: B05  \n")));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn status_lines_go_only_to_the_full_log() {
    let (opener, mut ends, _) = wire();
    let (handle, _task) = spawn(with_port(), opener, Arc::new(NoHost));
    let mut controller = Controller::new(ends.recv().await.unwrap());
    let mut pushes = handle.subscribe();
    controller.say(&["<Idle,MPos:0.00,0.00,0.00,WPos:0.000,0.000,0.000>"]).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    let feeds: Vec<Value> = std::iter::from_fn(|| pushes.try_recv().ok())
        .filter(|push| push.ns == Namespace::Logs)
        .map(|push| push.data["log"].clone())
        .collect();
    assert_eq!(feeds, [json!("log")]);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_closed_port_disconnects_and_is_reopened_after_5_s() {
    let (opener, mut ends, opened) = wire();
    let (handle, _task) = spawn(with_port(), opener, Arc::new(NoHost));
    let controller = Controller::new(ends.recv().await.unwrap());
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(handle.snapshot().link, LinkStatus::Connected);
    drop(controller);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(handle.snapshot().link, LinkStatus::Disconnected);
    let _again = ends.recv().await.unwrap();
    assert_eq!(opened.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_write_to_a_dead_port_disconnects() {
    let (opener, mut ends, _) = wire();
    let (handle, _task) = spawn(Echo { seen: Arc::default(), inner: with_port() }, opener, Arc::new(NoHost));
    handle.send(Event::Action(action("open")));
    drop(ends.recv().await.unwrap());
    handle.send(Event::Action(action("move")));
    let reopened = tokio::time::timeout(Duration::from_secs(1), ends.recv()).await;
    assert!(reopened.is_err(), "Echo never reopens; the shell just stops writing");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_port_that_will_not_open_is_retried() {
    let (handle, _task) = spawn(with_port(), Box::new(NoPort), Arc::new(NoHost));
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert_eq!(handle.snapshot().link, LinkStatus::Disconnected);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn system_actions_reach_the_host() {
    let host = Arc::new(Recorder::default());
    let (handle, _task) = spawn(Machine::new("h", Settings::defaults()), Box::new(NoPort), host.clone());
    let mut pushes = handle.subscribe();
    ["shutdown", "TurnOffRPI", "clearLogs", "updatePorts"]
        .into_iter()
        .for_each(|c| handle.send(Event::Action(action(c))));
    let ports = loop {
        let push = pushes.recv().await.unwrap();
        if push.data["command"] == "updatePorts" {
            break push.data["data"].clone();
        }
    };
    assert_eq!(ports, json!("[\"/dev/ttyACM0\"]"));
    assert_eq!(host.calls(), ["exit", "power_off", "clear_logs", "list_ports"]);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn health_goes_out_every_5_s_with_the_hosts_cpu_load() {
    let (handle, _task) =
        spawn(Machine::new("h", Settings::defaults()), Box::new(NoPort), Arc::new(Recorder::default()));
    let mut pushes = handle.subscribe();
    let started = tokio::time::Instant::now();
    let health = loop {
        let push = pushes.recv().await.unwrap();
        if push.data["command"] == "healthMessage" {
            break serde_json::from_str::<Value>(push.data["data"].as_str().unwrap()).unwrap();
        }
    };
    assert_eq!(started.elapsed().as_secs(), 5);
    assert_eq!(health["cpuUsage"], 12.5);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pushes_reach_every_subscriber() {
    let (handle, _task) = spawn(Machine::new("h", Settings::defaults()), Box::new(NoPort), Arc::new(NoHost));
    let (mut first, mut second) = (handle.subscribe(), handle.subscribe());
    handle.send(Event::Action(action("home")));
    let push = first.recv().await.unwrap();
    assert_eq!(second.recv().await.unwrap(), push);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_task_ends_when_every_handle_is_gone() {
    let (handle, task) = spawn(Machine::new("h", Settings::defaults()), Box::new(NoPort), Arc::new(NoHost));
    let clone = handle.clone();
    drop(handle);
    clone.send(Event::Tick);
    assert_eq!(clone.snapshot().host_address, "h");
    drop(clone);
    tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap();
}

#[test]
fn the_null_host_and_port_do_nothing() {
    let host = NoHost;
    assert!(host.list_ports().is_empty());
    host.log("x");
    host.clear_logs();
    host.power_off();
    host.exit();
    assert_eq!(host.cpu_load(), 0.0);
    assert!(NoPort.open("/dev/ttyACM0").is_err());
}
