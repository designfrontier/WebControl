use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};
use wc_config::Settings;
use wc_protocol::Push;

use crate::files::{Reply, Request};
use crate::{Effect, Event, Machine, Snapshot};

const TICK: Duration = Duration::from_millis(10);
const HEALTH: Duration = Duration::from_secs(5);
const PUSH_BACKLOG: usize = 1024;

/// What the shell drives: the real `Machine`, or a stand-in in tests.
pub trait Core: Send + 'static {
    fn handle(&mut self, event: Event, now: Duration) -> Vec<Effect>;
    fn snapshot(&self) -> Snapshot;
}

impl Core for Machine {
    fn handle(&mut self, event: Event, now: Duration) -> Vec<Effect> {
        Machine::handle(self, event, now)
    }
    fn snapshot(&self) -> Snapshot {
        Machine::snapshot(self)
    }
}

/// An open serial port, split.
pub type Serial = (Box<dyn AsyncRead + Send + Unpin>, Box<dyn AsyncWrite + Send + Unpin>);

/// Opens the controller's port: the real serial device, or the simulator.
pub trait PortOpener: Send + 'static {
    fn open(&mut self, port: &str) -> std::io::Result<Serial>;
}

/// The machine's reach outside the port: the system it runs on and the log file.
pub trait Host: Send + Sync + 'static {
    fn list_ports(&self) -> Vec<String>;
    /// One line of the serial log, stamped and ending in `\n`, as the log viewer shows it.
    fn log(&self, line: &str);
    fn clear_logs(&self);
    fn power_off(&self);
    fn exit(&self);
    /// The busiest core's load in percent.
    fn cpu_load(&self) -> f64;
    fn save_settings(&self, settings: &Settings);
    /// Makes a gcode library directory; false if the name leaves the library or the directory cannot be made.
    fn create_directory(&self, name: &str) -> bool;
}

/// No controller at all: every open fails, as with nothing plugged in.
pub struct NoPort;

impl PortOpener for NoPort {
    fn open(&mut self, port: &str) -> std::io::Result<Serial> {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{port}: no such port")))
    }
}

/// A host that does nothing, for tests and tools.
pub struct NoHost;

impl Host for NoHost {
    fn list_ports(&self) -> Vec<String> {
        Vec::new()
    }
    fn log(&self, _: &str) {}
    fn clear_logs(&self) {}
    fn power_off(&self) {}
    fn exit(&self) {}
    fn cpu_load(&self) -> f64 {
        0.0
    }
    fn save_settings(&self, _: &Settings) {}
    fn create_directory(&self, _: &str) -> bool {
        false
    }
}

/// What reaches the machine task on its command channel.
enum Command {
    Event(Event),
    Request(Request, oneshot::Sender<Reply>),
}

/// How sessions and routes reach the machine task. Cloning it is cheap.
#[derive(Clone)]
pub struct MachineHandle {
    commands: mpsc::UnboundedSender<Command>,
    realtime: mpsc::UnboundedSender<Event>,
    pushes: broadcast::Sender<Push>,
    snapshot: watch::Receiver<Snapshot>,
}

impl MachineHandle {
    /// Stops go on the realtime channel, which the machine task always reads first.
    pub fn send(&self, event: Event) {
        let _ = match event {
            Event::Action(action) if action.is_realtime() => self.realtime.send(Event::Stop(action)).map_err(drop),
            other => self.commands.send(Command::Event(other)).map_err(drop),
        };
    }

    /// Asks the machine and waits for its answer; `Failed` once the machine task has ended.
    pub async fn request(&self, request: Request) -> Reply {
        let (answer, reply) = oneshot::channel();
        if self.commands.send(Command::Request(request, answer)).is_err() {
            return Reply::Failed;
        }
        reply.await.unwrap_or(Reply::Failed)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Push> {
        self.pushes.subscribe()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }
}

/// A controller line or the end of the port, tagged with the port it came from.
type Read = (u64, Event);

struct Task<C: Core> {
    core: C,
    opener: Box<dyn PortOpener>,
    host: Arc<dyn Host>,
    writer: Option<Box<dyn AsyncWrite + Send + Unpin>>,
    reader: Option<JoinHandle<()>>,
    generation: u64,
    reads: mpsc::UnboundedSender<Read>,
    pushes: broadcast::Sender<Push>,
    snapshot: watch::Sender<Snapshot>,
    started: Instant,
    requests: HashMap<u64, oneshot::Sender<Reply>>,
    next_request: u64,
}

async fn read_lines(port: Box<dyn AsyncRead + Send + Unpin>, generation: u64, reads: mpsc::UnboundedSender<Read>) {
    let mut lines = BufReader::new(port).split(b'\n');
    while let Ok(Some(line)) = lines.next_segment().await {
        let text = String::from_utf8_lossy(&line).trim_end_matches('\r').to_string();
        if reads.send((generation, Event::ControllerLine(text))).is_err() {
            return;
        }
    }
    let _ = reads.send((generation, Event::PortClosed));
}

impl<C: Core> Task<C> {
    fn close(&mut self) {
        self.writer = None;
        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
        self.generation += 1;
    }

    fn open(&mut self, port: String) -> Event {
        match self.opener.open(&port) {
            Ok((reader, writer)) => {
                self.close();
                self.writer = Some(writer);
                self.reader = Some(tokio::spawn(read_lines(reader, self.generation, self.reads.clone())));
                Event::PortOpened { port }
            }
            Err(error) => Event::PortFailed { port, error: error.to_string() },
        }
    }

    fn log(&self, line: &str) {
        let stamp = format!("{:.2}: {line}\n", self.started.elapsed().as_secs_f64());
        self.host.log(&stamp);
        let feeds = if line.starts_with(['<', '[']) { &["log"][..] } else { &["alog", "log"][..] };
        feeds.iter().for_each(|feed| {
            let _ = self.pushes.send(Push::log(json!({"log": feed, "data": stamp, "state": false})));
        });
    }

    async fn write(&mut self, bytes: &[u8]) -> Option<Event> {
        let writer = self.writer.as_mut()?;
        let written = async {
            writer.write_all(bytes).await?;
            writer.flush().await
        };
        written.await.err().map(|_| Event::PortClosed)
    }

    async fn step(&mut self, event: Event) {
        let mut pending = VecDeque::from([event]);
        while let Some(event) = pending.pop_front() {
            let effects = self.core.handle(event, self.started.elapsed());
            for effect in effects {
                let follow_up = match effect {
                    Effect::Write(bytes) | Effect::Realtime(bytes) => self.write(&bytes).await,
                    Effect::Push(push) => {
                        let _ = self.pushes.send(push);
                        None
                    }
                    Effect::OpenPort(port) => Some(self.open(port)),
                    Effect::ClosePort => {
                        self.close();
                        None
                    }
                    Effect::ListPorts => Some(Event::Ports(self.host.list_ports())),
                    Effect::Log(line) => {
                        self.log(&line);
                        None
                    }
                    Effect::ClearLogs => {
                        self.host.clear_logs();
                        None
                    }
                    Effect::PowerOff => {
                        self.host.power_off();
                        None
                    }
                    Effect::Exit => {
                        self.host.exit();
                        None
                    }
                    Effect::SaveSettings(settings) => {
                        self.host.save_settings(&settings);
                        None
                    }
                    Effect::CreateDirectory(name) => {
                        self.host.create_directory(&name).then_some(Event::DirectoryCreated(name))
                    }
                    Effect::Reply { id, reply } => {
                        if let Some(answer) = self.requests.remove(&id) {
                            let _ = answer.send(reply);
                        }
                        None
                    }
                };
                pending.extend(follow_up);
            }
        }
        let next = self.core.snapshot();
        self.snapshot.send_if_modified(|current| std::mem::replace(current, next) != *current);
    }
}

/// Starts the machine task; it ends once every `MachineHandle` is dropped.
pub fn spawn<C: Core>(core: C, opener: Box<dyn PortOpener>, host: Arc<dyn Host>) -> (MachineHandle, JoinHandle<()>) {
    let (commands, mut command_rx) = mpsc::unbounded_channel();
    let (realtime, mut realtime_rx) = mpsc::unbounded_channel();
    let (reads, mut read_rx) = mpsc::unbounded_channel::<Read>();
    let (pushes, _) = broadcast::channel(PUSH_BACKLOG);
    let (snapshot_tx, snapshot) = watch::channel(core.snapshot());
    let handle = MachineHandle { commands, realtime, pushes: pushes.clone(), snapshot };
    let mut task = Task {
        core,
        opener,
        host,
        writer: None,
        reader: None,
        generation: 0,
        reads,
        pushes,
        snapshot: snapshot_tx,
        started: Instant::now(),
        requests: HashMap::new(),
        next_request: 0,
    };
    let join = tokio::spawn(async move {
        let mut clock = tokio::time::interval(TICK);
        clock.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut health = tokio::time::interval_at(Instant::now() + HEALTH, HEALTH);
        loop {
            let event = tokio::select! {
                biased;
                Some(stop) = realtime_rx.recv() => stop,
                Some((generation, read)) = read_rx.recv() => match generation == task.generation {
                    true => read,
                    false => continue,
                },
                command = command_rx.recv() => match command {
                    Some(Command::Event(event)) => event,
                    Some(Command::Request(request, answer)) => {
                        task.next_request += 1;
                        task.requests.insert(task.next_request, answer);
                        Event::Request { id: task.next_request, request }
                    }
                    None => break,
                },
                _ = health.tick() => Event::Health { cpu: task.host.cpu_load() },
                _ = clock.tick() => Event::Tick,
            };
            task.step(event).await;
        }
        task.close();
    });
    (handle, join)
}
