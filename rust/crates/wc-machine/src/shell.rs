use std::time::Duration;

use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};
use wc_protocol::Push;

use crate::{Effect, Event, Machine, Snapshot};

const TICK: Duration = Duration::from_millis(10);
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

/// The write half of the serial port.
pub trait PortWriter: Send + 'static {
    fn write(&mut self, bytes: &[u8]);
}

/// How sessions and routes reach the machine task. Cloning it is cheap.
#[derive(Clone)]
pub struct MachineHandle {
    commands: mpsc::UnboundedSender<Event>,
    realtime: mpsc::UnboundedSender<Event>,
    pushes: broadcast::Sender<Push>,
    snapshot: watch::Receiver<Snapshot>,
}

impl MachineHandle {
    /// Stops go on the realtime channel, which the machine task always reads first.
    pub fn send(&self, event: Event) {
        let (channel, event) = match event {
            Event::Action(action) if action.is_realtime() => (&self.realtime, Event::Stop(action)),
            other => (&self.commands, other),
        };
        let _ = channel.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Push> {
        self.pushes.subscribe()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }
}

struct Task<C: Core> {
    core: C,
    port: Option<Box<dyn PortWriter>>,
    pushes: broadcast::Sender<Push>,
    snapshot: watch::Sender<Snapshot>,
    started: Instant,
}

impl<C: Core> Task<C> {
    fn step(&mut self, event: Event) {
        let effects = self.core.handle(event, self.started.elapsed());
        effects.into_iter().for_each(|effect| match effect {
            Effect::Write(bytes) | Effect::Realtime(bytes) => {
                if let Some(port) = self.port.as_mut() {
                    port.write(&bytes);
                }
            }
            Effect::Push(push) => {
                let _ = self.pushes.send(push);
            }
        });
        let next = self.core.snapshot();
        self.snapshot.send_if_modified(|current| std::mem::replace(current, next) != *current);
    }
}

/// Starts the machine task; it ends once every `MachineHandle` is dropped.
pub fn spawn<C: Core>(core: C, port: Option<Box<dyn PortWriter>>) -> (MachineHandle, JoinHandle<()>) {
    let (commands, mut command_rx) = mpsc::unbounded_channel();
    let (realtime, mut realtime_rx) = mpsc::unbounded_channel();
    let (pushes, _) = broadcast::channel(PUSH_BACKLOG);
    let (snapshot_tx, snapshot) = watch::channel(core.snapshot());
    let handle = MachineHandle { commands, realtime, pushes: pushes.clone(), snapshot };
    let mut task = Task { core, port, pushes, snapshot: snapshot_tx, started: Instant::now() };
    let join = tokio::spawn(async move {
        let mut clock = tokio::time::interval(TICK);
        clock.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let event = tokio::select! {
                biased;
                Some(stop) = realtime_rx.recv() => stop,
                command = command_rx.recv() => match command {
                    Some(event) => event,
                    None => break,
                },
                _ = clock.tick() => Event::Tick,
            };
            task.step(event);
        }
    });
    (handle, join)
}
