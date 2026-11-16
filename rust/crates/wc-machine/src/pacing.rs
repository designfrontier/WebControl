//! Line pacing (`spec/firmware.md` B3), adjustable for the owner's loss-rate experiment, and its statistics: per
//! job and per segment (one pacing, from a change or the job's start), each ended with a `PACING` summary line.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use wc_config::Settings;
use wc_firmware::Sent;
use wc_protocol::main::{ControllerMessage, PacingSegment, PacingStats};

use crate::job::Flow;
use crate::machine::{Effect, Machine, broadcast};

pub const SECTION: &str = "WebControl Settings";
pub const MODE_KEY: &str = "linePacingMode";
pub const GAP_KEY: &str = "linePacingGapMs";
pub const MAX_GAP_MS: u64 = 500;
pub const DEFAULT_GAP_MS: u64 = 50;
/// Ended segments kept for the run panel's history.
pub const HISTORY: usize = 10;
pub const PUSH_EVERY: Duration = Duration::from_secs(2);
/// A line acked within the gap plus this ran no long move, so the gap before it is pacing overhead.
pub const SHORT_LINE_MARGIN: Duration = Duration::from_millis(100);
/// Gaps at or past this many milliseconds share the histogram's last bucket.
const GAP_BUCKETS: usize = 2001;
/// Unacked job lines remembered for the short-line gaps; a buffered controller holds far fewer.
const PENDING_LIMIT: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// A line due under the gap after the last write waits a full gap from when it became due.
    #[default]
    V094,
    /// A line waits only until the gap has passed since the last write.
    MinimumGap,
}

impl Mode {
    pub fn setting(self) -> &'static str {
        match self {
            Mode::V094 => "v0.94",
            Mode::MinimumGap => "minimum gap",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Mode::V094 => "v0.94",
            Mode::MinimumGap => "minimum_gap",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacing {
    pub mode: Mode,
    pub gap: Duration,
}

impl Default for Pacing {
    fn default() -> Pacing {
        Pacing { mode: Mode::V094, gap: Duration::from_millis(DEFAULT_GAP_MS) }
    }
}

impl Pacing {
    /// `mode=minimum_gap gap_ms=25`, as every `PACING` log line names a pacing.
    fn fields(self) -> String {
        format!("mode={} gap_ms={}", self.mode.slug(), self.gap.as_millis())
    }

    /// The settings' pacing; a value the settings screens would refuse reads as its default.
    pub fn read(settings: &Settings) -> Pacing {
        let mode = match settings.value(SECTION, MODE_KEY).and_then(|v| v.as_str()) {
            Some("minimum gap") => Mode::MinimumGap,
            _ => Mode::V094,
        };
        let gap = settings
            .value(SECTION, GAP_KEY)
            .and_then(wc_config::python_float)
            .filter(|ms| (0.0..=MAX_GAP_MS as f64).contains(ms))
            .map_or(DEFAULT_GAP_MS, |ms| ms.round() as u64);
        Pacing { mode, gap: Duration::from_millis(gap) }
    }

    /// Whether a line may go now; v0.94's wait is fixed in `paced_until` when the line first comes due.
    pub fn due(self, last_write: Option<Duration>, paced_until: &mut Option<Duration>, now: Duration) -> bool {
        let recent = last_write.is_some_and(|at| now < at + self.gap);
        match self.mode {
            Mode::V094 => {
                if paced_until.is_none() && recent {
                    *paced_until = Some(now + self.gap);
                }
                paced_until.is_none_or(|until| now >= until)
            }
            Mode::MinimumGap => !recent,
        }
    }
}

/// One segment's or one job's counts.
#[derive(Debug, Clone)]
struct Tally {
    pacing: Pacing,
    lines: u64,
    lost: u64,
    lost_twice: u64,
    elapsed: Duration,
    streaming_since: Option<Duration>,
    recovery: Duration,
    /// Short-line gaps by whole millisecond.
    gaps: Vec<u32>,
    first_line: Option<usize>,
    last_line: Option<usize>,
}

impl Tally {
    fn new(pacing: Pacing) -> Tally {
        Tally {
            pacing,
            lines: 0,
            lost: 0,
            lost_twice: 0,
            elapsed: Duration::ZERO,
            streaming_since: None,
            recovery: Duration::ZERO,
            gaps: vec![0; GAP_BUCKETS],
            first_line: None,
            last_line: None,
        }
    }

    fn streaming(&mut self, streaming: bool, now: Duration) {
        match (streaming, self.streaming_since) {
            (true, None) => self.streaming_since = Some(now),
            (false, Some(since)) => {
                self.elapsed += now.saturating_sub(since);
                self.streaming_since = None;
            }
            _ => {}
        }
    }

    fn elapsed(&self, now: Duration) -> Duration {
        self.elapsed + self.streaming_since.map_or(Duration::ZERO, |since| now.saturating_sub(since))
    }

    fn median_gap_ms(&self) -> Option<f64> {
        let count: u64 = self.gaps.iter().map(|&n| u64::from(n)).sum();
        if count == 0 {
            return None;
        }
        let half = count.div_ceil(2);
        let mut seen = 0u64;
        let median = self.gaps.iter().position(|&n| {
            seen += u64::from(n);
            seen >= half
        });
        median.map(|ms| ms as f64)
    }

    fn report(&self, now: Duration, ended_by: Option<&str>) -> PacingSegment {
        let elapsed = self.elapsed(now).as_secs_f64();
        let lines = self.lines as f64;
        let per = |value: f64, over: f64| (over > 0.0).then(|| value / over);
        PacingSegment {
            mode: self.pacing.mode.setting().into(),
            gap_ms: self.pacing.gap.as_millis() as u64,
            lines: self.lines,
            lost: self.lost,
            lost_twice: self.lost_twice,
            elapsed_s: round(elapsed, 1000.0),
            recovery_s: round(self.recovery.as_secs_f64(), 1000.0),
            lines_per_minute: per(lines * 60.0, elapsed).filter(|_| self.lines > 0).map(|n| round(n, 10.0)),
            seconds_per_line: per(elapsed, lines).map(|n| round(n, 1000.0)),
            lost_percent: per(self.lost as f64 * 100.0, lines).map(|n| round(n, 100.0)),
            median_short_gap_ms: self.median_gap_ms(),
            ended_by: ended_by.map(String::from),
        }
    }

    /// The stable `key=value` line the serial log, the service log and the console get.
    fn summary(&self, label: &str, now: Duration, ended_by: &str) -> String {
        let report = self.report(now, Some(ended_by));
        let number = |value: Option<f64>| value.map_or("none".to_string(), |n| n.to_string());
        let line = |index: Option<usize>| index.map_or("none".to_string(), |i| (i + 1).to_string());
        format!(
            "PACING {label} {} lines={} lost={} lost_twice={} lost_pct={} lines_per_min={} \
             s_per_line={} recovery_s={} median_short_gap_ms={} duration_s={} file_line_start={} file_line_end={} \
             ended_by={}",
            self.pacing.fields(),
            report.lines,
            report.lost,
            report.lost_twice,
            number(report.lost_percent),
            number(report.lines_per_minute),
            number(report.seconds_per_line),
            report.recovery_s,
            number(report.median_short_gap_ms),
            report.elapsed_s,
            line(self.first_line),
            line(self.last_line),
            slug(ended_by),
        )
    }
}

fn slug(text: &str) -> String {
    text.replace(' ', "_").to_lowercase()
}

fn round(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

/// A job line not yet acked: its id, when it went out and how long after the write before it.
#[derive(Debug, Clone, Copy)]
struct Pending {
    id: u64,
    written: Duration,
    gap: Option<Duration>,
}

#[derive(Debug, Clone)]
pub(crate) struct Stats {
    pacing: Pacing,
    in_job: bool,
    job: Option<Tally>,
    segment: Option<Tally>,
    history: VecDeque<PacingSegment>,
    pending: VecDeque<Pending>,
    next_push: Duration,
    /// Set by Stop, so the segment it ends says so.
    stopping: bool,
    /// Something changed since the last push.
    dirty: bool,
    job_ended_by: &'static str,
    /// What the last push said; `GET /api/state` and `GET /api/run/pacing` read it.
    pub shown: Arc<PacingStats>,
}

impl Stats {
    pub fn new(pacing: Pacing) -> Stats {
        Stats {
            pacing,
            in_job: false,
            job: None,
            segment: None,
            history: VecDeque::new(),
            pending: VecDeque::new(),
            next_push: Duration::ZERO,
            stopping: false,
            dirty: false,
            job_ended_by: "job end",
            shown: Arc::new(PacingStats {
                mode: pacing.mode.setting().into(),
                gap_ms: pacing.gap.as_millis() as u64,
                job: None,
                segment: None,
                history: Vec::new(),
            }),
        }
    }

    pub fn pacing(&self) -> Pacing {
        self.pacing
    }

    fn report(&self, now: Duration) -> PacingStats {
        PacingStats {
            mode: self.pacing.mode.setting().into(),
            gap_ms: self.pacing.gap.as_millis() as u64,
            job: self.job.as_ref().map(|job| job.report(now, (!self.in_job).then_some(self.job_ended_by))),
            segment: self.segment.as_ref().map(|segment| segment.report(now, None)),
            history: self.history.iter().cloned().collect(),
        }
    }

    fn tallies(&mut self) -> impl Iterator<Item = &mut Tally> {
        self.job.iter_mut().chain(self.segment.iter_mut())
    }

    pub fn stopping(&mut self) {
        self.stopping |= self.in_job;
    }
}

/// Every place a message goes so a log alone tells the segments apart.
fn announce(text: String, effects: &mut Vec<Effect>) {
    tracing::info!("{text}");
    effects.push(Effect::Log(text.clone()));
    effects.push(broadcast(&ControllerMessage(text)));
}

impl Machine {
    /// Before and after each pump: a pacing change or a job's start or end opens or closes a segment.
    pub(crate) fn pacing_observe(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let pacing = Pacing::read(&self.settings);
        let in_job = self.job.flow.in_job();
        let file_line = self.index + 1;
        if pacing != self.stats.pacing {
            let old = self.stats.pacing;
            self.stats.pacing = pacing;
            self.controller.paced_until = None;
            self.stats.dirty = true;
            if self.stats.in_job && in_job {
                self.end_segment("pacing change", now, effects);
                let from = format!("from_mode={} from_gap_ms={}", old.mode.slug(), old.gap.as_millis());
                announce(format!("PACING CHANGE file_line={file_line} {} {from}", pacing.fields()), effects);
                self.stats.segment = Some(Tally::new(pacing));
            }
        }
        if in_job && !self.stats.in_job {
            announce(format!("PACING JOB START file_line={file_line} {}", pacing.fields()), effects);
            self.stats.job = Some(Tally::new(pacing));
            self.stats.segment = Some(Tally::new(pacing));
            self.stats.pending.clear();
            self.stats.in_job = true;
            self.stats.dirty = true;
        }
        let streaming = matches!(self.job.flow, Flow::Streaming | Flow::Draining);
        self.stats.tallies().for_each(|tally| tally.streaming(streaming, now));
        if !in_job && self.stats.in_job {
            let why = if std::mem::take(&mut self.stats.stopping) { "Stop" } else { "job end" };
            self.end_segment(why, now, effects);
            self.stats.in_job = false;
            self.stats.job_ended_by = why;
            let last = self.stats.job.as_ref().and_then(|job| job.last_line).map_or(0, |index| index + 1);
            if let Some(job) = &self.stats.job {
                announce(job.summary("JOB", now, why), effects);
            }
            let fields = pacing.fields();
            announce(format!("PACING JOB END file_line_end={last} {fields} ended_by={}", slug(why)), effects);
            self.stats.dirty = true;
        }
    }

    /// After the pump, so a push counts the lines it wrote.
    pub(crate) fn push_pacing(&mut self, now: Duration, effects: &mut Vec<Effect>) {
        let due = self.stats.in_job && now >= self.stats.next_push;
        if !std::mem::take(&mut self.stats.dirty) && !due {
            return;
        }
        self.stats.next_push = now + PUSH_EVERY;
        let report = self.stats.report(now);
        effects.push(broadcast(&report));
        self.stats.shown = Arc::new(report);
    }

    pub(crate) fn pacing_report(&self, now: Duration) -> PacingStats {
        self.stats.report(now)
    }

    fn end_segment(&mut self, why: &str, now: Duration, effects: &mut Vec<Effect>) {
        let Some(mut segment) = self.stats.segment.take() else { return };
        segment.streaming(false, now);
        announce(segment.summary("SEGMENT", now, why), effects);
        self.stats.history.push_front(segment.report(now, Some(why)));
        self.stats.history.truncate(HISTORY);
    }

    /// A job line went out; `previous` is when the write before it did.
    pub(crate) fn pacing_written(&mut self, id: u64, index: usize, previous: Option<Duration>, now: Duration) {
        if !self.stats.in_job {
            return;
        }
        self.stats.tallies().for_each(|tally| {
            tally.lines += 1;
            tally.first_line = tally.first_line.or(Some(index));
            tally.last_line = Some(index);
        });
        // Only a write in this stretch of streaming: the wait before a job or across a pause is no pacing.
        let since = self.stats.job.as_ref().and_then(|job| job.streaming_since);
        let gap = previous.filter(|at| since.is_some_and(|since| *at >= since)).map(|at| now.saturating_sub(at));
        self.stats.pending.push_back(Pending { id, written: now, gap });
        if self.stats.pending.len() > PENDING_LIMIT {
            self.stats.pending.pop_front();
        }
    }

    pub(crate) fn pacing_acked(&mut self, sent: Option<&Sent>, now: Duration) {
        let Some(sent) = sent else { return };
        let Some(at) = self.stats.pending.iter().position(|pending| pending.id == sent.id) else { return };
        let Some(pending) = self.stats.pending.remove(at) else { return };
        let short = now.saturating_sub(pending.written) < self.stats.pacing.gap + SHORT_LINE_MARGIN;
        if let Some(gap) = pending.gap.filter(|_| short) {
            let bucket = (gap.as_millis() as usize).min(GAP_BUCKETS - 1);
            self.stats.tallies().for_each(|tally| tally.gaps[bucket] += 1);
        }
    }

    /// Job lines the controller never received; `resent` when they go out again, not when the job pauses.
    pub(crate) fn pacing_lost(&mut self, lost: &[Sent], resent: bool, twice: bool, now: Duration) {
        let lines = lost.iter().filter(|sent| sent.index.is_some()).count() as u64;
        if !self.stats.in_job || lines == 0 {
            return;
        }
        let ids: Vec<u64> = lost.iter().map(|sent| sent.id).collect();
        let written = self.stats.pending.iter().filter(|p| ids.contains(&p.id)).map(|p| p.written).min();
        self.stats.pending.retain(|pending| !ids.contains(&pending.id));
        let recovery = written.filter(|_| resent).map_or(Duration::ZERO, |at| now.saturating_sub(at));
        self.stats.tallies().for_each(|tally| {
            tally.lost += lines;
            tally.lost_twice += u64::from(twice);
            tally.recovery += recovery;
        });
    }

    /// For a client that missed the pushes.
    pub(crate) fn pacing_shown(&self) -> Effect {
        broadcast(&*self.stats.shown)
    }
}
