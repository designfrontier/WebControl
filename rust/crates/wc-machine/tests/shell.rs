use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use wc_machine::{Core, Effect, Event, Machine, PortWriter, Snapshot, spawn};
use wc_protocol::{Action, Envelope, Push};

fn action(command: &str) -> Action {
    Action { command: command.into(), arg: json!(""), arg1: json!("") }
}

/// Records what it handled; answers every event with a write and a push naming it.
struct Recorder {
    seen: Arc<Mutex<Vec<String>>>,
    inner: Machine,
}

fn name(event: &Event) -> String {
    match event {
        Event::Action(a) | Event::Stop(a) => a.command.clone(),
        Event::ClientConnected { sid } => format!("connect {sid}"),
        Event::ControllerLine(line) => format!("rx {line}"),
        Event::Tick => "tick".into(),
    }
}

impl Core for Recorder {
    fn handle(&mut self, event: Event, _: Duration) -> Vec<Effect> {
        let label = name(&event);
        if label == "tick" {
            self.seen.lock().unwrap().push(label);
            return Vec::new();
        }
        self.seen.lock().unwrap().push(label.clone());
        let write = match event {
            Event::Stop(_) => Effect::Realtime(b"!\n".to_vec()),
            _ => Effect::Write(format!("{label}\n").into_bytes()),
        };
        vec![write, Effect::Push(Push::message(Envelope::int("showFPSpinner", 1)))]
    }
    fn snapshot(&self) -> Snapshot {
        self.inner.snapshot()
    }
}

#[derive(Clone, Default)]
struct Port(Arc<Mutex<Vec<u8>>>);

impl PortWriter for Port {
    fn write(&mut self, bytes: &[u8]) {
        self.0.lock().unwrap().extend_from_slice(bytes);
    }
}

fn recorder() -> (Recorder, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    (Recorder { seen: seen.clone(), inner: Machine::new("h") }, seen)
}

async fn stop_first(round: usize) {
    let (core, seen) = recorder();
    let port = Port::default();
    let (handle, task) = spawn(core, Some(Box::new(port.clone())));
    ["move", "moveZ", "home"].into_iter().for_each(|c| handle.send(Event::Action(action(c))));
    handle.send(Event::Action(action("stopRun")));
    tokio::time::sleep(Duration::from_millis(1)).await;
    let handled: Vec<String> = seen.lock().unwrap().iter().filter(|s| *s != "tick").cloned().collect();
    assert_eq!(handled, ["stopRun", "move", "moveZ", "home"], "round {round}");
    assert_eq!(String::from_utf8(port.0.lock().unwrap().clone()).unwrap(), "!\nmove\nmoveZ\nhome\n");
    drop(handle);
    task.await.unwrap();
}

/// Repeated because an unbiased select would still put Stop first now and then.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stop_jumps_the_queue_of_commands() {
    for round in 0..32 {
        stop_first(round).await;
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pushes_reach_every_subscriber() {
    let (handle, _task) = spawn(Machine::new("10.0.0.5:5000"), None);
    let (mut first, mut second) = (handle.subscribe(), handle.subscribe());
    handle.send(Event::Action(action("home")));
    let push = first.recv().await.unwrap();
    assert!(push.data["data"].as_str().unwrap().contains("Function not currently implemented"));
    assert_eq!(second.recv().await.unwrap(), push);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_clock_ticks_every_10_ms() {
    let (core, seen) = recorder();
    let (_handle, _task) = spawn(core, None);
    tokio::time::sleep(Duration::from_millis(55)).await;
    let ticks = seen.lock().unwrap().iter().filter(|s| *s == "tick").count();
    assert!((5..=7).contains(&ticks), "{ticks} ticks");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn writes_without_a_port_are_dropped_and_the_snapshot_is_readable() {
    let (core, seen) = recorder();
    let (handle, _task) = spawn(core, None);
    handle.send(Event::ControllerLine("ok".into()));
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert!(seen.lock().unwrap().contains(&"rx ok".to_string()));
    assert_eq!(handle.snapshot().host_address, "h");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_task_ends_when_every_handle_is_gone() {
    let (handle, task) = spawn(Machine::new("h"), None);
    let clone = handle.clone();
    drop(handle);
    clone.send(Event::Tick);
    drop(clone);
    tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap();
}
