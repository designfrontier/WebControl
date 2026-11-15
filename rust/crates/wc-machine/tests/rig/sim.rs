//! A `Machine` wired to a `wc-sim` controller with no IO: what the machine writes reaches the simulator at once,
//! and what the simulator prints comes back as controller lines, on a shared fake clock.

use std::collections::VecDeque;
use std::time::Duration;

use serde_json::json;
use wc_config::Settings;
use wc_machine::{Effect, Event, Machine};
use wc_sim::{Build, Sim};

use super::{action, settings};

#[derive(Clone)]
pub struct SimRig {
    pub machine: Machine,
    pub sim: Sim,
    pub now: Duration,
    /// Every effect since the last `take`.
    pub effects: Vec<Effect>,
    /// Every line written, realtime included, without its `\n`.
    pub written: Vec<String>,
}

impl SimRig {
    /// A Z motor attached, so Z moves need no hand adjustment, unless `settings` says otherwise.
    pub fn with(settings: Settings) -> SimRig {
        let mut rig = SimRig {
            machine: Machine::new("10.0.0.5:5000", settings),
            sim: Sim::new(Build::holey_51_27()),
            now: Duration::ZERO,
            effects: Vec::new(),
            written: Vec::new(),
        };
        rig.send(Event::PortOpened { port: "/dev/ttyACM0".into() });
        let boot = rig.sim.boot(rig.millis());
        rig.deliver(boot);
        rig.run(2000);
        rig.take();
        rig
    }

    pub fn new() -> SimRig {
        SimRig::with(settings(&[("Maslow Settings", "zAxis", json!(1))]))
    }

    fn millis(&self) -> u64 {
        self.now.as_millis() as u64
    }

    /// Hands the machine an event, then plays out the conversation it starts with the simulator.
    pub fn send(&mut self, event: Event) {
        let mut pending = VecDeque::from([event]);
        while let Some(event) = pending.pop_front() {
            let effects = self.machine.handle_inline(event, self.now);
            effects.iter().for_each(|effect| {
                if let Effect::Write(bytes) | Effect::Realtime(bytes) = effect {
                    self.written.push(String::from_utf8_lossy(bytes).trim_end_matches('\n').to_string());
                    let replies = self.sim.receive(bytes, self.millis());
                    pending.extend(replies.into_iter().map(Event::ControllerLine));
                }
            });
            self.effects.extend(effects);
        }
    }

    pub fn deliver(&mut self, lines: Vec<String>) {
        lines.into_iter().for_each(|line| self.send(Event::ControllerLine(line)));
    }

    pub fn act(&mut self, command: &str) {
        self.act_with(command, json!(""), json!(""));
    }

    pub fn act_with(&mut self, command: &str, arg: serde_json::Value, arg1: serde_json::Value) {
        self.send(action(command, arg, arg1));
    }

    /// Lets `ms` pass in 10 ms ticks, the simulator first in each.
    pub fn run(&mut self, ms: u64) {
        (0..ms.div_ceil(10)).for_each(|_| {
            self.now += Duration::from_millis(10);
            let lines = self.sim.tick(self.millis());
            self.deliver(lines);
            self.send(Event::Tick);
        });
    }

    pub fn take(&mut self) -> (Vec<Effect>, Vec<String>) {
        (std::mem::take(&mut self.effects), std::mem::take(&mut self.written))
    }
}
