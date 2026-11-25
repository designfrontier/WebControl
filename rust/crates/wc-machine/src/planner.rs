//! A planner build (`spec/firmware.md` D, the firmware's `PROTOCOL.md`): every line through the buffer goes numbered
//! and checksummed, its copy is kept until a cumulative `ok N<n>`, and an `rs N<n>` or a lost ack sends the lines
//! again from there.

use std::collections::VecDeque;
use std::time::Duration;

use wc_firmware::planner::{DRAIN, PLANNER_DEPTH, numbered, renumber};
use wc_firmware::{EMPTY_RING, Planned, Sent};

use crate::machine::{Effect, Machine};

/// No `ok N`, `rs` or write for this long, then a report with the ring empty, means a line or its ack was lost.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(1);
/// File lines whose numbers are kept, for the line a report names; far more than the ring and planner hold.
const HISTORY: usize = 256;

#[derive(Debug, Clone)]
struct Line {
    number: u64,
    text: String,
    index: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct Numbered {
    next: u64,
    /// `M110` set the firmware's count since the boot, the stop or the job's start.
    synced: bool,
    /// Lines not acked yet, oldest first; the first `in_flight` went out since the last resend.
    unacked: VecDeque<Line>,
    in_flight: usize,
    /// An `rs` came: the copies before it still count until the next `ok N`, and only the resent line goes.
    stale: bool,
    last_heard: Duration,
    planned: Option<Planned>,
    history: VecDeque<(u64, usize)>,
    shown: Option<usize>,
}

impl Default for Numbered {
    fn default() -> Numbered {
        Numbered {
            next: 1,
            synced: false,
            unacked: VecDeque::new(),
            in_flight: 0,
            stale: false,
            last_heard: Duration::ZERO,
            planned: None,
            history: VecDeque::new(),
            shown: None,
        }
    }
}

impl Numbered {
    /// A `!` or an alarm emptied the planner and the ring; the firmware keeps its count and numbered mode.
    pub fn flushed(&mut self) {
        self.unacked.clear();
        self.in_flight = 0;
        self.stale = false;
        self.synced = false;
    }

    /// A job starts: the next line goes after an `M110`.
    pub fn renumber(&mut self) {
        self.synced = false;
        self.shown = None;
    }

    /// Nothing new may go: lines wait to go again, or only the resent one may be in flight.
    pub fn holds(&self) -> bool {
        self.stale || self.in_flight < self.unacked.len()
    }

    fn acked_through(&mut self, number: u64) {
        let done = self.unacked.iter().take_while(|line| line.number <= number).count();
        self.unacked.drain(..done);
        self.in_flight = self.in_flight.saturating_sub(done);
    }

    /// The file line the latest report says runs now.
    fn reported(&self) -> Option<usize> {
        let line = self.planned.map(|planned| planned.line).filter(|&line| line > 0)?;
        self.history.iter().rev().find(|(number, _)| *number == line).map(|&(_, index)| index)
    }

    /// The file line running now, else the oldest the planner has not taken.
    pub fn running(&self) -> Option<usize> {
        self.reported().or_else(|| self.unacked.iter().find_map(|line| line.index))
    }

    /// A line's bytes with its number and checksum (at most `*255`), and the `M110` before it if one is due.
    pub fn framed_len(&self, line: &str) -> usize {
        let renumbering = if self.synced { 0 } else { renumber(self.next).len() + 1 };
        format!("N{} ", self.next).len() + line.trim().len() + "*255\n".len() + renumbering
    }
}

fn written(line: &str, effects: &mut Vec<Effect>) {
    effects.push(Effect::Write(format!("{line}\n").into_bytes()));
    effects.push(Effect::Log(format!("Sent: {line}")));
}

impl Machine {
    /// `text` numbered, with an `M110` first when the count must be set; returns its accounting id.
    pub(crate) fn write_numbered(
        &mut self,
        text: &str,
        index: Option<usize>,
        now: Duration,
        effects: &mut Vec<Effect>,
    ) -> u64 {
        let numbering = &mut self.controller.numbered;
        let number = numbering.next;
        numbering.next += 1;
        numbering.unacked.push_back(Line { number, text: text.into(), index });
        numbering.in_flight += 1;
        if let Some(index) = index {
            numbering.history.push_back((number, index));
            if numbering.history.len() > HISTORY {
                numbering.history.pop_front();
            }
        }
        self.transmit(number, text, index, now, effects)
    }

    fn transmit(
        &mut self,
        number: u64,
        text: &str,
        index: Option<usize>,
        now: Duration,
        effects: &mut Vec<Effect>,
    ) -> u64 {
        let controller = &mut self.controller;
        if !std::mem::replace(&mut controller.numbered.synced, true) {
            let line = renumber(number);
            controller.accounting.sent(&format!("{line}\n"), None);
            written(&line, effects);
        }
        let framed = numbered(number, text);
        controller.numbered.last_heard = now;
        written(&framed, effects);
        controller.accounting.sent_numbered(&format!("{text}\n"), framed.len() + 1, index, number)
    }

    /// The lines not acked go out again, oldest first: after an `rs` only the first, after a lost ack while they fit.
    pub(crate) fn resend(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        while let Some(line) = self.controller.numbered.unacked.get(self.controller.numbered.in_flight).cloned() {
            let numbering = &self.controller.numbered;
            let fits = self.controller.accounting.space() > (numbered(line.number, &line.text).len() + 1) as i64;
            if (numbering.stale && numbering.in_flight > 0) || (!numbering.stale && !fits) {
                return;
            }
            self.controller.numbered.in_flight += 1;
            let id = self.transmit(line.number, &line.text, line.index, now, effects);
            self.controller.ready = false;
            self.controller.last_write = Some(now);
            if let Some(probe) = self.controller.probe.as_mut().filter(|_| line.text.contains("G38.2")) {
                probe.id = id;
            }
        }
    }

    /// `ok N<n>`, or an `error:` for the line resent after an `rs`: the copies it covers are freed.
    pub(crate) fn numbered_ack(&mut self, number: Option<u64>, now: Duration) -> Vec<Sent> {
        let controller = &mut self.controller;
        controller.numbered.last_heard = now;
        let freed = match (std::mem::take(&mut controller.numbered.stale), number) {
            (true, _) => controller.accounting.take(),
            (false, Some(number)) => controller.accounting.acked_through(number),
            (false, None) => Vec::from_iter(controller.accounting.acked()),
        };
        if let Some(number) = number.or_else(|| freed.last().and_then(|sent| sent.number)) {
            controller.numbered.acked_through(number);
        }
        freed
    }

    /// An `error:` freed `sent`, which acks every numbered line up to it.
    pub(crate) fn numbered_freed(&mut self, sent: Option<&Sent>) {
        if let Some(number) = sent.and_then(|sent| sent.number) {
            self.controller.numbered.acked_through(number);
        }
    }

    /// `rs N<n>`: the firmware dropped everything from line `n` on.
    pub(crate) fn resend_requested(&mut self, from: u64, now: Duration) {
        if !self.planner {
            return;
        }
        let numbering = &mut self.controller.numbered;
        numbering.acked_through(from.saturating_sub(1));
        if numbering.unacked.front().is_none_or(|line| line.number != from) {
            tracing::warn!(from, "the controller asked for a line WebControl no longer holds; renumbering");
            numbering.synced = false;
        }
        numbering.stale = true;
        numbering.in_flight = 0;
        numbering.last_heard = now;
        self.pacing_resent();
    }

    /// A planner build's `[PE:]`: the line it runs, and whether an ack is overdue.
    pub(crate) fn planner_report(
        &mut self,
        planned: Option<Planned>,
        space: i64,
        now: Duration,
        effects: &mut Vec<Effect>,
    ) {
        let Some(planned) = planned.filter(|_| self.planner) else { return };
        self.controller.numbered.planned = Some(planned);
        if self.ack_overdue(planned, space, now) {
            tracing::warn!("no ack and the ring empty; sending the lines not acked again");
            let numbering = &mut self.controller.numbered;
            numbering.stale = false;
            numbering.in_flight = 0;
            numbering.last_heard = now;
            self.controller.accounting.clear();
            self.controller.ready = true;
            self.pacing_resent();
        }
        let running = self.controller.numbered.reported().filter(|_| self.job.flow.in_job());
        if running.is_some() && running != std::mem::replace(&mut self.controller.numbered.shown, running) {
            running.into_iter().for_each(|index| self.push_position_of(index, effects));
        }
    }

    /// A pause the firmware holds, or a `G4 P0` waiting for the moves before it, is no lost ack.
    fn ack_overdue(&self, planned: Planned, space: i64, now: Duration) -> bool {
        let draining = planned.queued > 0
            && self.controller.accounting.lines().is_some_and(|lines| {
                lines.first().and_then(|sent| sent.text.as_deref()).is_some_and(|text| text.trim() == DRAIN)
            });
        let waiting = self.job.firmware_paused || self.job.manual_z.is_some() || draining;
        !self.controller.accounting.is_empty()
            && now.saturating_sub(self.controller.numbered.last_heard) >= ACK_TIMEOUT
            && space == EMPTY_RING
            && planned.queued < PLANNER_DEPTH
            && !waiting
    }

    /// Where a job resumes after a halt: the line running, else the oldest the controller has not answered.
    pub(crate) fn running_index(&self) -> Option<usize> {
        match self.planner {
            true => self.controller.numbered.running(),
            false => self.controller.accounting.oldest_index(),
        }
    }
}
