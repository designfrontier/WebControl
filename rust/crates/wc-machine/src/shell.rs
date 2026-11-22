use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{OwnedMutexGuard, broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};
use wc_config::Settings;
use wc_protocol::Push;

use crate::files::{Reply, Request};
use crate::gpio::{Button, Leds};
use crate::{Effect, Event, FirmwareJob, JobResult, Machine, Response, Snapshot};

const TICK: Duration = Duration::from_millis(10);
const HEALTH: Duration = Duration::from_secs(5);
const PUSH_BACKLOG: usize = 1024;
/// How long a closed port may take to write what was queued before it, such as the `!` before a flash.
const CLOSE_GRACE: Duration = Duration::from_millis(250);

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

/// Opens the controller's port: the real serial device, or the simulator. Called on the blocking pool.
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
    /// Writes `webcontrol.json`; called off the machine task, so a slow card never delays Stop.
    fn save_settings(&self, _settings: &Settings) {}
    /// Makes a gcode library directory; false if the name leaves the library or the directory cannot be made.
    fn create_directory(&self, name: &str) -> bool;
    /// Lights the GPIO status LEDs.
    fn leds(&self, _leds: Leds) {}
    /// Runs avrdude or saves a firmware backup, off the machine task.
    fn firmware(&self, job: FirmwareJob) -> JobResult {
        unsupported(job)
    }
}

/// What a host that cannot flash answers.
pub fn unsupported(job: FirmwareJob) -> JobResult {
    let why = "this host cannot run avrdude".to_string();
    match job {
        FirmwareJob::ReadFlash { .. } => JobResult::Read(Err(why)),
        FirmwareJob::Save { .. } => JobResult::Saved(Err(why)),
        FirmwareJob::WriteFlash { .. } => JobResult::Written { success: false, output: why },
    }
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
    fn create_directory(&self, _: &str) -> bool {
        false
    }
}

/// What reaches the machine task on its command channel.
enum Command {
    Event(Event),
    Request(Request, oneshot::Sender<Reply>),
    Edit(crate::Request, oneshot::Sender<Response>),
}

/// How sessions and routes reach the machine task. Cloning it is cheap.
#[derive(Clone)]
pub struct MachineHandle {
    commands: mpsc::UnboundedSender<Command>,
    realtime: mpsc::UnboundedSender<Event>,
    pushes: broadcast::Sender<Push>,
    snapshot: watch::Receiver<Snapshot>,
    vision: broadcast::Sender<crate::optical::Vision>,
    queue: broadcast::Sender<crate::queue::Outbound>,
}

impl MachineHandle {
    /// Stops go on the realtime channel, which the machine task always reads first.
    pub fn send(&self, event: Event) {
        let _ = match event {
            Event::Action(action) if action.is_realtime() => self.realtime.send(Event::Stop(action)).map_err(drop),
            Event::Button(Button::Stop) => self.realtime.send(Event::Button(Button::Stop)).map_err(drop),
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

    /// Asks the machine without waiting, for callers that cannot await, such as a `Drop`.
    pub fn submit(&self, request: Request) {
        let _ = self.commands.send(Command::Request(request, oneshot::channel().0));
    }

    /// A push about state the machine does not hold, such as the tool library.
    pub fn broadcast(&self, push: Push) {
        let _ = self.pushes.send(push);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Push> {
        self.pushes.subscribe()
    }

    /// What the optical scan asks of the camera; whoever serves it answers with `Event::Seen`.
    pub fn vision(&self) -> broadcast::Receiver<crate::optical::Vision> {
        self.vision.subscribe()
    }

    /// The job queue's loads and saves; whoever serves them answers loads as `/openGCode` does.
    pub fn queue_requests(&self) -> broadcast::Receiver<crate::queue::Outbound> {
        self.queue.subscribe()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    /// The snapshot as it changes, for a reader that would otherwise poll.
    pub fn watch(&self) -> watch::Receiver<Snapshot> {
        self.snapshot.clone()
    }

    /// Sends a settings write to the machine and waits for its answer; `None` once the machine task has ended.
    pub async fn edit(&self, request: crate::Request) -> Option<Response> {
        let (answer, response) = oneshot::channel();
        self.commands.send(Command::Edit(request, answer)).ok()?;
        response.await.ok()
    }
}

/// A port event or controller line, tagged with the port it came from.
type Read = (u64, Event);

/// What the machine writes, as the port's writer task takes it.
enum Out {
    Line(Vec<u8>),
    Realtime(Vec<u8>),
}

impl Out {
    fn stops(&self) -> bool {
        matches!(self, Out::Realtime(bytes) if bytes.starts_with(b"!"))
    }
}

/// Held by whoever has the controller's device open, or is opening it: a port task, or avrdude.
type PortLock = Arc<tokio::sync::Mutex<()>>;

/// An open (or opening) port: the writer's queue, and the task that opens, reads and writes it.
struct Port {
    out: mpsc::UnboundedSender<Out>,
    task: JoinHandle<()>,
}

struct Task<C: Core> {
    core: C,
    opener: Arc<Mutex<Box<dyn PortOpener>>>,
    host: Arc<dyn Host>,
    port: Option<Port>,
    device: PortLock,
    generation: u64,
    reads: mpsc::UnboundedSender<Read>,
    pushes: broadcast::Sender<Push>,
    snapshot: watch::Sender<Snapshot>,
    started: Instant,
    persist: watch::Sender<Option<Arc<Settings>>>,
    jobs: mpsc::UnboundedSender<Event>,
    requests: HashMap<u64, oneshot::Sender<Reply>>,
    responses: HashMap<u64, oneshot::Sender<Response>>,
    next_request: u64,
    directories: mpsc::UnboundedSender<String>,
    vision: broadcast::Sender<crate::optical::Vision>,
    queue: broadcast::Sender<crate::queue::Outbound>,
}

/// Makes each directory in turn off the machine task, so a stalled card never holds up a Stop.
async fn make_directories(
    host: Arc<dyn Host>,
    mut names: mpsc::UnboundedReceiver<String>,
    made: mpsc::UnboundedSender<Event>,
) {
    while let Some(name) = names.recv().await {
        let host = host.clone();
        let created = tokio::task::spawn_blocking(move || host.create_directory(&name).then_some(name)).await;
        if let Ok(Some(name)) = created {
            let _ = made.send(Event::DirectoryCreated(name));
        }
    }
}

/// The next thing to write: a realtime byte before any line. A `!` drops the lines queued before it, which the
/// controller would have flushed had they reached it.
fn next_out(queued: &mut VecDeque<Out>) -> Option<Out> {
    let Some(at) = queued.iter().position(|out| matches!(out, Out::Realtime(_))) else {
        return queued.pop_front();
    };
    if queued[at].stops() && at > 0 {
        tracing::warn!(dropped = at, "lines queued behind a Stop were never written");
        queued.drain(..at);
        return queued.pop_front();
    }
    queued.remove(at)
}

/// Writes what the machine sends until the port fails or closes. A `!` cuts short a line still being written.
/// Never flushes: on a serial port that is `tcdrain`, which blocks until the line has left at 57600 baud.
async fn write_port(
    mut port: Box<dyn AsyncWrite + Send + Unpin>,
    mut outgoing: mpsc::UnboundedReceiver<Out>,
    generation: u64,
    reads: mpsc::UnboundedSender<Read>,
) {
    let mut queued = VecDeque::new();
    loop {
        while let Ok(out) = outgoing.try_recv() {
            queued.push_back(out);
        }
        let written = match next_out(&mut queued) {
            None => match outgoing.recv().await {
                Some(out) => {
                    queued.push_back(out);
                    continue;
                }
                None => return,
            },
            Some(Out::Realtime(bytes)) => port.write_all(&bytes).await,
            Some(Out::Line(bytes)) => {
                let write = port.write_all(&bytes);
                tokio::pin!(write);
                loop {
                    tokio::select! {
                        biased;
                        Some(out) = outgoing.recv() => {
                            let stops = out.stops();
                            queued.push_back(out);
                            if stops {
                                tracing::warn!("a Stop cut short the line being written");
                                break Ok(());
                            }
                        }
                        written = &mut write => break written,
                    }
                }
            }
        };
        if written.is_err() {
            let _ = reads.send((generation, Event::PortClosed));
            return;
        }
    }
}

/// Opens `name` on the blocking pool, then reads and writes it until the machine closes it and the writer has
/// emptied its queue. The device lock goes with the open, so it is released only after the port is.
async fn run_port(
    opener: Arc<Mutex<Box<dyn PortOpener>>>,
    device: PortLock,
    name: String,
    outgoing: mpsc::UnboundedReceiver<Out>,
    generation: u64,
    reads: mpsc::UnboundedSender<Read>,
) {
    let (port, held) = (name.clone(), device.lock_owned().await);
    let opened =
        tokio::task::spawn_blocking(move || (opener.lock().unwrap_or_else(PoisonError::into_inner).open(&port), held))
            .await;
    let (opened, _held): (_, Option<OwnedMutexGuard<()>>) = match opened {
        Ok((opened, held)) => (opened, Some(held)),
        Err(panic) => (Err(std::io::Error::other(panic)), None),
    };
    match opened {
        Ok((reader, writer)) => {
            let _ = reads.send((generation, Event::PortOpened { port: name }));
            let writing = write_port(writer, outgoing, generation, reads.clone());
            tokio::pin!(writing);
            tokio::select! {
                () = read_lines(reader, generation, reads) => writing.await,
                () = &mut writing => {}
            }
        }
        Err(error) => {
            let _ = reads.send((generation, Event::PortFailed { port: name, error: error.to_string() }));
        }
    }
}

/// Runs `work` on the blocking pool and hands its event back to the machine task.
fn off_task(events: &mpsc::UnboundedSender<Event>, work: impl FnOnce() -> Event + Send + 'static) {
    let events = events.clone();
    tokio::spawn(async move {
        if let Ok(event) = tokio::task::spawn_blocking(work).await {
            let _ = events.send(event);
        }
    });
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

/// Lets a closed port write what is queued, then drops it.
async fn finish(mut task: JoinHandle<()>) {
    if tokio::time::timeout(CLOSE_GRACE, &mut task).await.is_err() {
        task.abort();
    }
}

impl<C: Core> Task<C> {
    fn close(&mut self) {
        if let Some(Port { out, task }) = self.port.take() {
            drop(out);
            tokio::spawn(finish(task));
        }
        self.generation += 1;
    }

    /// Writes made before the port has opened go out once it has.
    fn open(&mut self, name: String) {
        self.close();
        let (out, outgoing) = mpsc::unbounded_channel();
        let (opener, device) = (self.opener.clone(), self.device.clone());
        let opening = run_port(opener, device, name, outgoing, self.generation, self.reads.clone());
        self.port = Some(Port { out, task: tokio::spawn(opening) });
    }

    fn log(&self, line: &str) {
        let stamp = format!("{:.2}: {line}\n", self.started.elapsed().as_secs_f64());
        self.host.log(&stamp);
        let feeds = if line.starts_with(['<', '[']) { &["log"][..] } else { &["alog", "log"][..] };
        feeds.iter().for_each(|feed| {
            let _ = self.pushes.send(Push::log(json!({"log": feed, "data": stamp, "state": false})));
        });
    }

    fn write(&self, out: Out) {
        if let Some(port) = &self.port {
            let _ = port.out.send(out);
        }
    }

    fn effect(&mut self, effect: Effect) {
        match effect {
            Effect::Write(bytes) => self.write(Out::Line(bytes)),
            Effect::Realtime(bytes) => self.write(Out::Realtime(bytes)),
            Effect::Push(push) => drop(self.pushes.send(push)),
            Effect::OpenPort(port) => self.open(port),
            Effect::ClosePort => self.close(),
            Effect::ListPorts => {
                let host = self.host.clone();
                off_task(&self.jobs, move || Event::Ports(host.list_ports()));
            }
            Effect::Log(line) => self.log(&line),
            Effect::ClearLogs => self.host.clear_logs(),
            Effect::PowerOff => self.host.power_off(),
            Effect::Exit => self.host.exit(),
            Effect::Respond { id, response } => {
                if let Some(answer) = self.responses.remove(&id) {
                    let _ = answer.send(response);
                }
            }
            Effect::SaveSettings(settings) => drop(self.persist.send_replace(Some(settings))),
            Effect::Firmware(job) => {
                let (host, jobs, device) = (self.host.clone(), self.jobs.clone(), self.device.clone());
                tokio::spawn(async move {
                    let held = match job {
                        FirmwareJob::Save { .. } => None,
                        _ => Some(device.lock_owned().await),
                    };
                    let result = tokio::task::spawn_blocking(move || (host.firmware(job), held).0).await;
                    let result = result.unwrap_or_else(|e| JobResult::Saved(Err(e.to_string())));
                    let _ = jobs.send(Event::FirmwareDone(result));
                });
            }
            Effect::CreateDirectory(name) => drop(self.directories.send(name)),
            Effect::Reply { id, reply } => {
                if let Some(answer) = self.requests.remove(&id) {
                    let _ = answer.send(reply);
                }
            }
            Effect::Leds(leds) => self.host.leds(leds),
            Effect::Compute(work) => off_task(&self.jobs, move || work.run()),
            Effect::Vision(request) => drop(self.vision.send(request)),
            Effect::Queue(request) => drop(self.queue.send(request)),
        }
    }

    fn step(&mut self, event: Event) {
        let effects = self.core.handle(event, self.started.elapsed());
        effects.into_iter().for_each(|effect| self.effect(effect));
        let next = self.core.snapshot();
        self.snapshot.send_if_modified(|current| std::mem::replace(current, next) != *current);
    }
}

/// Saves the newest settings handed to it, one at a time, until the machine task ends.
async fn persist(mut latest: watch::Receiver<Option<Arc<Settings>>>, host: Arc<dyn Host>) {
    while latest.changed().await.is_ok() {
        let Some(settings) = latest.borrow_and_update().clone() else {
            continue;
        };
        let host = host.clone();
        let _ = tokio::task::spawn_blocking(move || host.save_settings(&settings)).await;
    }
}

/// Starts the machine task; it ends once every `MachineHandle` is dropped, after the last save is written.
pub fn spawn<C: Core>(core: C, opener: Box<dyn PortOpener>, host: Arc<dyn Host>) -> (MachineHandle, JoinHandle<()>) {
    let (commands, mut command_rx) = mpsc::unbounded_channel();
    let (realtime, mut realtime_rx) = mpsc::unbounded_channel();
    let (reads, mut read_rx) = mpsc::unbounded_channel::<Read>();
    let (jobs, mut job_rx) = mpsc::unbounded_channel();
    let (directories, directory_rx) = mpsc::unbounded_channel();
    let (made, mut made_rx) = mpsc::unbounded_channel();
    tokio::spawn(make_directories(host.clone(), directory_rx, made));
    let (pushes, _) = broadcast::channel(PUSH_BACKLOG);
    let (snapshot_tx, snapshot) = watch::channel(core.snapshot());
    let (persist_tx, persist_rx) = watch::channel(None);
    let writer = tokio::spawn(persist(persist_rx, host.clone()));
    let (vision, _) = broadcast::channel(PUSH_BACKLOG);
    let (queue, _) = broadcast::channel(PUSH_BACKLOG);
    let handle = MachineHandle {
        commands,
        realtime,
        pushes: pushes.clone(),
        snapshot,
        vision: vision.clone(),
        queue: queue.clone(),
    };
    let mut task = Task {
        core,
        opener: Arc::new(Mutex::new(opener)),
        host,
        port: None,
        device: PortLock::default(),
        generation: 0,
        reads,
        pushes,
        snapshot: snapshot_tx,
        started: Instant::now(),
        persist: persist_tx,
        jobs,
        requests: HashMap::new(),
        responses: HashMap::new(),
        next_request: 0,
        directories,
        vision,
        queue,
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
                Some(done) = job_rx.recv() => done,
                Some(made) = made_rx.recv() => made,
                command = command_rx.recv() => match command {
                    Some(Command::Event(event)) => event,
                    Some(Command::Request(request, answer)) => {
                        task.next_request += 1;
                        task.requests.insert(task.next_request, answer);
                        Event::Request { id: task.next_request, request }
                    }
                    Some(Command::Edit(request, answer)) => {
                        task.next_request += 1;
                        task.responses.insert(task.next_request, answer);
                        Event::Edit { id: task.next_request, request }
                    }
                    None => break,
                },
                _ = health.tick() => {
                    let host = task.host.clone();
                    off_task(&task.jobs, move || Event::Health { cpu: host.cpu_load() });
                    continue;
                }
                _ = clock.tick() => Event::Tick,
            };
            task.step(event);
        }
        task.close();
        drop(task);
        let _ = writer.await;
    });
    (handle, join)
}
