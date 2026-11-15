//! CPU work the machine hands the shell to run on the blocking pool, so a Stop never waits behind it. One of each
//! kind runs at a time; the newest request waits behind it and the ones it replaced are dropped.

use std::sync::Arc;

use wc_gcode::position;
use wc_kinematics::Kinematics;
use wc_protocol::main::{ErrorValueMessage, GcodePositionMessage};

use crate::machine::{Effect, Event, Machine, alert, broadcast};

/// Tells the UI there is no forward-kinematics position (wart 6).
pub(crate) const NO_COMPUTED_POSITION: f64 = -999999.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Work {
    /// The forward-kinematics solve for an error report's computed X and Y.
    ComputedPosition { model: Box<Kinematics>, chains: (f64, f64), guess: (f64, f64), report: ErrorValueMessage },
    /// `sendGCodePositionUpdate`: the file read from the top to `index`. `failure` is alerted if that fails.
    GcodePosition { gcode: Arc<Vec<String>>, index: usize, scale: f64, failure: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Done {
    /// The report to push, and the model with the solver's tilt as the solve left it.
    ComputedPosition {
        model: Box<Kinematics>,
        report: ErrorValueMessage,
    },
    GcodePosition(Result<GcodePositionMessage, Option<String>>),
}

impl Work {
    pub fn run(self) -> Event {
        Event::Computed(match self {
            Work::ComputedPosition { mut model, chains: (left, right), guess, mut report } => {
                let computed = model.forward_from(left, right, guess);
                (report.computed_x, report.computed_y) =
                    computed.unwrap_or((NO_COMPUTED_POSITION, NO_COMPUTED_POSITION));
                Done::ComputedPosition { model, report }
            }
            Work::GcodePosition { gcode, index, scale, failure } => {
                let at = position::find_position_at(&gcode, index).map_err(|_| failure);
                Done::GcodePosition(at.map(|at| GcodePositionMessage {
                    xval: at.x * scale,
                    yval: at.y * scale,
                    zval: at.z * scale,
                    gcode_line: gcode.get(index).cloned().unwrap_or_default(),
                    gcode_line_index: index,
                }))
            }
        })
    }
}

/// One kind of work: whether it is running, and the request waiting behind it.
#[derive(Debug, Clone, Default)]
struct Lane {
    running: bool,
    next: Option<Work>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Lanes {
    position: Lane,
    gcode: Lane,
}

impl Lanes {
    fn of(&mut self, work: &Work) -> &mut Lane {
        match work {
            Work::ComputedPosition { .. } => &mut self.position,
            Work::GcodePosition { .. } => &mut self.gcode,
        }
    }
}

impl Machine {
    pub(crate) fn compute(&mut self, work: Work, effects: &mut Vec<Effect>) {
        let lane = self.lanes.of(&work);
        if lane.running {
            lane.next = Some(work);
        } else {
            lane.running = true;
            effects.push(Effect::Compute(work));
        }
    }

    pub(crate) fn computed(&mut self, done: Done, effects: &mut Vec<Effect>) {
        let lane = match &done {
            Done::ComputedPosition { model, report } => {
                // A `$` write since the solve began changed the model; keep the new one.
                if model.geometry() == self.kinematics.geometry() {
                    self.kinematics = (**model).clone();
                }
                effects.push(broadcast(report));
                &mut self.lanes.position
            }
            Done::GcodePosition(result) => {
                match result {
                    Ok(message) => effects.push(broadcast(message)),
                    Err(Some(failure)) => effects.push(alert(failure)),
                    Err(None) => {}
                }
                &mut self.lanes.gcode
            }
        };
        let Some(mut next) = lane.next.take() else {
            lane.running = false;
            return;
        };
        if let Work::ComputedPosition { model, .. } = &mut next {
            **model = self.kinematics.clone();
        }
        effects.push(Effect::Compute(next));
    }
}
