//! Stop latency: a Stop issued while slow work is under way reaches the port, or is handled, within `BOUND` of
//! real time. Each piece of slow work has a fake that takes `SLOW`, or real work that takes about as long.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, ready};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncWrite, AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc;
use wc_config::Settings;
use wc_machine::{Event, Host, Machine, MachineHandle, PortOpener, Serial, spawn};
use wc_protocol::Action;
use wc_protocol::main::LinkStatus;

const BOUND: Duration = Duration::from_millis(5);
const SLOW: Duration = Duration::from_millis(300);
/// Long enough for the machine task to have started the slow work before the Stop is sent.
const SETTLE: Duration = Duration::from_millis(10);

type Arrivals = Arc<Mutex<Vec<(Instant, Vec<u8>)>>>;

/// One scenario at a time, so another's slow work never competes for the cores being timed.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The controller's side of a port: when each write arrived, and how slow the port is.
struct Tap {
    arrivals: Arrivals,
    /// A line's write stays pending this long, as with the kernel's buffer full.
    stall: Duration,
    stalled: Option<Pin<Box<tokio::time::Sleep>>>,
    /// A flush blocks this long after a line, as `tcdrain` does while the line goes out at 57600 baud.
    drain: Duration,
    undrained: bool,
}

fn realtime(bytes: &[u8]) -> bool {
    bytes.starts_with(b"!") || bytes.starts_with(b"~")
}

impl AsyncWrite for Tap {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        if !realtime(buf) && !self.stall.is_zero() {
            let stall = self.stall;
            ready!(self.stalled.get_or_insert_with(|| Box::pin(tokio::time::sleep(stall))).as_mut().poll(cx));
            self.stalled = None;
        }
        self.undrained = !realtime(buf);
        self.arrivals.lock().unwrap().push((Instant::now(), buf.to_vec()));
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if std::mem::take(&mut self.undrained) {
            std::thread::sleep(self.drain);
        }
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Default, Clone, Copy)]
struct Slowness {
    open: Duration,
    stall: Duration,
    drain: Duration,
    list: Duration,
}

struct Opener {
    slowness: Slowness,
    arrivals: Arrivals,
    controllers: mpsc::UnboundedSender<DuplexStream>,
}

impl PortOpener for Opener {
    fn open(&mut self, _: &str) -> std::io::Result<Serial> {
        std::thread::sleep(self.slowness.open);
        let (ours, theirs) = tokio::io::duplex(4096);
        let _ = self.controllers.send(theirs);
        let Slowness { stall, drain, .. } = self.slowness;
        let tap = Tap { arrivals: self.arrivals.clone(), stall, stalled: None, drain, undrained: false };
        Ok((Box::new(ours), Box::new(tap)))
    }
}

struct SlowHost(Duration);

impl Host for SlowHost {
    fn list_ports(&self) -> Vec<String> {
        std::thread::sleep(self.0);
        vec!["/dev/ttyACM0".into()]
    }
    fn log(&self, _: &str) {}
    fn clear_logs(&self) {}
    fn power_off(&self) {}
    fn exit(&self) {}
    fn cpu_load(&self) -> f64 {
        0.0
    }
    fn create_directory(&self, _: &str) -> bool {
        false
    }
}

struct Rig {
    handle: MachineHandle,
    arrivals: Arrivals,
    controllers: mpsc::UnboundedReceiver<DuplexStream>,
}

fn machine(changes: &[(&str, &str, Value)]) -> Machine {
    let mut settings = Settings::defaults();
    settings.set_value("Maslow Settings", "COMport", json!("/dev/ttyACM0"));
    changes.iter().for_each(|(section, key, value)| assert!(settings.set_value(section, key, value.clone())));
    Machine::new("h", settings)
}

fn start(machine: Machine, slowness: Slowness) -> Rig {
    let arrivals = Arrivals::default();
    let (sender, controllers) = mpsc::unbounded_channel();
    let opener = Opener { slowness, arrivals: arrivals.clone(), controllers: sender };
    let (handle, _task) = spawn(machine, Box::new(opener), Arc::new(SlowHost(slowness.list)));
    Rig { handle, arrivals, controllers }
}

fn command(name: &str, arg: Value) -> Event {
    let action = Action { command: name.into(), arg, arg1: json!("") };
    if action.is_realtime() { Event::Stop(action) } else { Event::Action(action) }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

impl Rig {
    /// The port open and the controller's end in hand; nothing from the controller yet.
    async fn connected(machine: Machine, slowness: Slowness) -> (Rig, DuplexStream) {
        let mut rig = start(machine, slowness);
        let controller = rig.controllers.recv().await.unwrap();
        let handle = rig.handle.clone();
        until("the link", || handle.snapshot().link == LinkStatus::Connected).await;
        (rig, controller)
    }

    fn written(&self, bytes: &[u8]) -> bool {
        self.arrivals.lock().unwrap().iter().any(|(_, written)| written.starts_with(bytes))
    }

    /// From sending Stop, `SETTLE` after the slow work began (or after `written` reached the port), to its `!`
    /// reaching the port.
    async fn stop_reaches_the_port(&self, written: Option<&'static [u8]>) -> Duration {
        let sent = stop_soon(&self.handle, written.map(|bytes| (self.arrivals.clone(), bytes)));
        let sent = tokio::task::spawn_blocking(move || sent.join().unwrap()).await.unwrap();
        let stop = || self.arrivals.lock().unwrap().iter().find(|(at, b)| *at >= sent && b.starts_with(b"!")).cloned();
        until("the stop", || stop().is_some()).await;
        stop().unwrap().0 - sent
    }
}

/// Sends Stop from a thread of its own, so a blocked runtime cannot hold back the sending.
fn stop_soon(handle: &MachineHandle, after: Option<(Arrivals, &'static [u8])>) -> std::thread::JoinHandle<Instant> {
    let handle = handle.clone();
    std::thread::spawn(move || {
        if let Some((arrivals, bytes)) = after {
            while !arrivals.lock().unwrap().iter().any(|(_, written)| written.starts_with(bytes)) {
                std::thread::sleep(Duration::from_micros(100));
            }
        }
        std::thread::sleep(SETTLE);
        let sent = Instant::now();
        handle.send(command("stopRun", json!("")));
        sent
    })
}

fn within_bound(work: &str, latency: Duration) {
    eprintln!("stop latency during {work}: {latency:?}");
    assert!(latency < BOUND, "a Stop during {work} took {latency:?} to reach the port");
}

async fn say(controller: &mut DuplexStream, lines: &[&str]) {
    let text: String = lines.iter().map(|line| format!("{line}\r\n")).collect();
    controller.write_all(text.as_bytes()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_during_the_computed_position_solve_reaches_the_port_in_time() {
    let _alone = ONE_AT_A_TIME.lock().await;
    let quadrilateral = machine(&[
        ("WebControl Settings", "computedPosition", json!(1)),
        ("Computed Settings", "kinematicsTypeComputed", json!("1")),
    ]);
    let (rig, mut controller) = Rig::connected(quadrilateral, Slowness::default()).await;
    // An unreachable pair leaves the quadrilateral solver tilted, so the next solve takes about 40 ms.
    say(&mut controller, &["[PE:0.1,-1.0,120,10.0,9000.0]", "[PE:0.2,-1.0,120,3000.0,3000.0]"]).await;
    within_bound("the forward-kinematics solve", rig.stop_reaches_the_port(None).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_during_a_gcode_goto_reaches_the_port_in_time() {
    let _alone = ONE_AT_A_TIME.lock().await;
    let mut loaded = machine(&[]);
    let lines: String = (0..60_000).map(|i| format!("G1 X{}.{} Y{} Z-0.1 F500\n", i % 100, i % 7, i % 50)).collect();
    loaded.load_gcode(&lines);
    let (rig, _controller) = Rig::connected(loaded, Slowness::default()).await;
    rig.handle.send(command("moveGcodeGoto", json!(59_990)));
    within_bound("the gcode position rescan", rig.stop_reaches_the_port(None).await);
}

async fn stop_during_a_slow_line(slowness: Slowness, work: &str) {
    let (rig, mut controller) = Rig::connected(machine(&[]), slowness).await;
    say(&mut controller, &["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"]).await;
    within_bound(work, rig.stop_reaches_the_port(Some(b"B05")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_during_a_draining_line_reaches_the_port_in_time() {
    let _alone = ONE_AT_A_TIME.lock().await;
    stop_during_a_slow_line(Slowness { drain: SLOW, ..Slowness::default() }, "a line draining to the port").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_during_a_stalled_line_reaches_the_port_in_time() {
    let _alone = ONE_AT_A_TIME.lock().await;
    let stalled = Slowness { stall: SLOW, ..Slowness::default() };
    let (rig, mut controller) = Rig::connected(machine(&[]), stalled).await;
    say(&mut controller, &["PCB v1.6 TLE9201  Detected", "Grbl v1.00", "ready", "ok"]).await;
    within_bound("a line stalled on a full port", rig.stop_reaches_the_port(None).await);
    tokio::time::sleep(SLOW).await;
    assert!(!rig.written(b"B05"), "the line the Stop cut short is never finished");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_during_port_listing_reaches_the_port_in_time() {
    let _alone = ONE_AT_A_TIME.lock().await;
    let (rig, _controller) = Rig::connected(machine(&[]), Slowness { list: SLOW, ..Slowness::default() }).await;
    rig.handle.send(command("updatePorts", json!("")));
    within_bound("the port listing", rig.stop_reaches_the_port(None).await);
}

/// Nothing is connected while the port opens, so the Stop is timed to the push it makes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_during_a_port_open_is_handled_in_time() {
    let _alone = ONE_AT_A_TIME.lock().await;
    let rig = start(machine(&[]), Slowness { open: SLOW, ..Slowness::default() });
    let mut pushes = rig.handle.subscribe();
    let sent = stop_soon(&rig.handle, None);
    let handled = std::thread::spawn(move || {
        while pushes.blocking_recv().unwrap().data["command"] != "clearAlarm" {}
        Instant::now()
    });
    let latency = handled.join().unwrap() - sent.join().unwrap();
    eprintln!("stop latency during the port open: {latency:?}");
    assert!(latency < BOUND, "a Stop during the port open took {latency:?} to be handled");
}
