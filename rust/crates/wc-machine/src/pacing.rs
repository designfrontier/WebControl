//! Line pacing (`spec/firmware.md` B3), adjustable for the owner's loss-rate experiment, and its statistics: per
//! job and per segment (one pacing, from a change or the job's start), each ended with a `PACING` summary line. The
//! job's totals double as the job timer: wall time, streaming (cutting) time and the loaded file's estimate left.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use wc_config::Settings;
use wc_firmware::{PositionError, Sent, Status};
use wc_protocol::main::{ControllerMessage, PacingSegment, PacingStats, PositionErrorStats};

use crate::job::Flow;
use crate::machine::{Effect, Machine, broadcast};

pub const SECTION: &str = "WebControl Settings";
pub const MODE_KEY: &str = "linePacingMode";
pub const GAP_KEY: &str = "linePacingGapMs";
/// Changed only through `PUT /api/run/pacing`, so it stays off the section forms.
pub const STATS_KEY: &str = "linePacingStats";
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
/// The share of a job's estimate done before its real pace rescales the time left.
const PACE_TRUSTED: f64 = 0.02;

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

/// Whether the run panel shows the pacing; off unless the setting says otherwise.
pub fn stats_shown(settings: &Settings) -> bool {
    matches!(settings.value(SECTION, STATS_KEY), Some(v) if v == 1 || v == true)
}

/// The larger chain error of each `[PE:]` report taken while the sled moved, and the overshoot the peaks show.
#[derive(Debug, Clone, Copy, Default)]
struct ErrorTally {
    samples: u64,
    sum_mm: f64,
    max_mm: f64,
    over_1mm: u64,
    max_ahead_mm: Option<f64>,
    overshoots: u64,
}

/// More than this ahead of the setpoint counts as an overshoot.
const OVERSHOOT_MM: f64 = 0.2;

impl ErrorTally {
    fn add(&mut self, mm: f64, max_mm: f64) {
        self.samples += 1;
        self.sum_mm += mm;
        self.max_mm = self.max_mm.max(max_mm);
        self.over_1mm += u64::from(max_mm > 1.0);
    }

    fn ahead(&mut self, mm: f64) {
        self.max_ahead_mm = Some(self.max_ahead_mm.unwrap_or(0.0).max(mm));
        self.overshoots += u64::from(mm > OVERSHOOT_MM);
    }

    fn report(self) -> Option<PositionErrorStats> {
        (self.samples > 0).then(|| PositionErrorStats {
            samples: self.samples,
            mean_mm: round(self.sum_mm / self.samples as f64, 1000.0),
            max_mm: round(self.max_mm, 1000.0),
            over_1mm: self.over_1mm,
            max_ahead_mm: self.max_ahead_mm.map(|mm| round(mm, 1000.0)),
            overshoots: self.max_ahead_mm.map(|_| self.overshoots),
        })
    }

    /// `mean_mm=0.412 max_mm=1.3 over_1mm=2 samples=180`, `none` for a mean and max with no samples, then
    /// `max_ahead_mm=0.61 overshoots=3` from a `peak-error` firmware.
    fn fields(self) -> String {
        let (mean, max) =
            self.report().map_or(("none".into(), "none".into()), |r| (r.mean_mm.to_string(), r.max_mm.to_string()));
        let ahead = self
            .max_ahead_mm
            .map_or(String::new(), |mm| format!(" max_ahead_mm={} overshoots={}", round(mm, 1000.0), self.overshoots));
        format!("mean_mm={mean} max_mm={max} over_1mm={} samples={}{ahead}", self.over_1mm, self.samples)
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
    started: Duration,
    ended: Option<Duration>,
    errors: ErrorTally,
}

impl Tally {
    fn new(pacing: Pacing, now: Duration) -> Tally {
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
            started: now,
            ended: None,
            errors: ErrorTally::default(),
        }
    }

    fn end(&mut self, now: Duration) {
        self.streaming(false, now);
        self.ended = Some(now);
    }

    fn wall(&self, now: Duration) -> Duration {
        self.ended.unwrap_or(now).saturating_sub(self.started)
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

    fn report(&self, now: Duration, ended_by: Option<&str>, remaining: Option<f64>) -> PacingSegment {
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
            wall_s: round(self.wall(now).as_secs_f64(), 1000.0),
            remaining_s: remaining,
            position_error: self.errors.report(),
        }
    }

    /// The stable `key=value` line the serial log, the service log and the console get.
    fn summary(&self, label: &str, now: Duration, ended_by: &str) -> String {
        let report = self.report(now, Some(ended_by), None);
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
    show: bool,
    in_job: bool,
    job: Option<Tally>,
    segment: Option<Tally>,
    history: VecDeque<PacingSegment>,
    pending: VecDeque<Pending>,
    next_push: Duration,
    /// Set by Stop, so the segment it ends says so.
    stopping: bool,
    /// Something changed since the last push.
    pub(crate) dirty: bool,
    job_ended_by: &'static str,
    /// The last status report's MPos, and whether it differed from the one before.
    mpos: Option<[f64; 3]>,
    moving: bool,
    /// What the last push said; `GET /api/state` and `GET /api/run/pacing` read it.
    pub shown: Arc<PacingStats>,
    /// The loaded file's estimated seconds through the end of each line.
    pub estimate: Vec<f32>,
}

impl Stats {
    pub fn new(pacing: Pacing, show: bool) -> Stats {
        Stats {
            pacing,
            show,
            in_job: false,
            job: None,
            segment: None,
            history: VecDeque::new(),
            pending: VecDeque::new(),
            next_push: Duration::ZERO,
            stopping: false,
            dirty: false,
            job_ended_by: "job end",
            mpos: None,
            moving: false,
            shown: Arc::new(PacingStats {
                mode: pacing.mode.setting().into(),
                gap_ms: pacing.gap.as_millis() as u64,
                job: None,
                segment: None,
                history: Vec::new(),
                estimate_s: None,
                show_stats: show,
            }),
            estimate: Vec::new(),
        }
    }

    pub fn pacing(&self) -> Pacing {
        self.pacing
    }

    fn report(&self, now: Duration, remaining: Option<f64>) -> PacingStats {
        let ended_by = (!self.in_job).then_some(self.job_ended_by);
        PacingStats {
            mode: self.pacing.mode.setting().into(),
            gap_ms: self.pacing.gap.as_millis() as u64,
            job: self.job.as_ref().map(|job| job.report(now, ended_by, remaining)),
            segment: self.segment.as_ref().map(|segment| segment.report(now, None, None)),
            history: self.history.iter().cloned().collect(),
            estimate_s: self.estimate.last().map(|&total| round(f64::from(total), 1.0)),
            show_stats: self.show,
        }
    }

    /// The estimate's seconds before 0-based `line`.
    fn estimated_before(&self, line: usize) -> f64 {
        line.checked_sub(1).and_then(|at| self.estimate.get(at)).map_or(0.0, |&s| f64::from(s))
    }

    fn tallies(&mut self) -> impl Iterator<Item = &mut Tally> {
        self.job.iter_mut().chain(self.segment.iter_mut())
    }

    pub fn stopping(&mut self) {
        self.stopping |= self.in_job;
    }
}

/// Every place a message goes so a log alone tells the segments apart.
pub(crate) fn announce(text: String, effects: &mut Vec<Effect>) {
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
        let show = stats_shown(&self.settings);
        if show != self.stats.show {
            self.stats.show = show;
            self.stats.dirty = true;
        }
        if pacing != self.stats.pacing {
            let old = self.stats.pacing;
            self.stats.pacing = pacing;
            self.controller.paced_until = None;
            self.stats.dirty = true;
            if self.stats.in_job && in_job {
                self.end_segment("pacing change", now, effects);
                let from = format!("from_mode={} from_gap_ms={}", old.mode.slug(), old.gap.as_millis());
                announce(format!("PACING CHANGE file_line={file_line} {} {from}", pacing.fields()), effects);
                self.stats.segment = Some(Tally::new(pacing, now));
            }
        }
        if in_job && !self.stats.in_job {
            announce(format!("PACING JOB START file_line={file_line} {}", pacing.fields()), effects);
            self.stats.job = Some(Tally::new(pacing, now));
            self.stats.segment = Some(Tally::new(pacing, now));
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
            let times = self.stats.job.as_mut().map_or(String::new(), |job| {
                job.end(now);
                announce(job.summary("JOB", now, why), effects);
                let seconds = |time: Duration| round(time.as_secs_f64(), 10.0);
                format!(" wall_s={} cutting_s={}", seconds(job.wall(now)), seconds(job.elapsed(now)))
            });
            let estimated =
                self.job_estimate().map_or("none".into(), |(total, start)| round(total - start, 10.0).to_string());
            let file = self.gcode_name().unwrap_or_else(|| "none".into());
            let fields =
                format!("{} ended_by={}{times} estimated_s={estimated} file={file}", pacing.fields(), slug(why));
            announce(format!("PACING JOB END file_line_end={last} {fields}"), effects);
            let errors = self.stats.job.as_ref().map_or_else(ErrorTally::default, |job| job.errors);
            announce(format!("PE JOB {} file={file}", errors.fields()), effects);
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
        let report = self.stats.report(now, self.remaining_s(now));
        effects.push(broadcast(&report));
        self.stats.shown = Arc::new(report);
    }

    pub(crate) fn pacing_report(&self, now: Duration) -> PacingStats {
        self.stats.report(now, self.remaining_s(now))
    }

    /// The loaded file's estimated total and the estimate before the job's first line.
    fn job_estimate(&self) -> Option<(f64, f64)> {
        let total = f64::from(*self.stats.estimate.last()?);
        let first = self.stats.job.as_ref().and_then(|job| job.first_line).unwrap_or(self.index);
        Some((total, self.stats.estimated_before(first)))
    }

    /// The running job's estimate left, scaled by its real pace once enough of it is done to judge.
    fn remaining_s(&self, now: Duration) -> Option<f64> {
        let job = self.stats.job.as_ref().filter(|_| self.stats.in_job)?;
        let (total, start) = self.job_estimate()?;
        let finished = self.stats.estimated_before(self.running_index().unwrap_or(self.index));
        let done = finished - start;
        let pace = if done > PACE_TRUSTED * (total - start) { job.elapsed(now).as_secs_f64() / done } else { 1.0 };
        Some(round((total - finished).max(0.0) * pace, 1.0))
    }

    fn end_segment(&mut self, why: &str, now: Duration, effects: &mut Vec<Effect>) {
        let Some(mut segment) = self.stats.segment.take() else { return };
        segment.end(now);
        announce(segment.summary("SEGMENT", now, why), effects);
        self.stats.history.push_front(segment.report(now, Some(why), None));
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

    /// A planner build asked for lines again, or their ack was lost: one loss each time.
    pub(crate) fn pacing_resent(&mut self) {
        if self.stats.in_job {
            self.stats.tallies().for_each(|tally| tally.lost += 1);
        }
    }

    pub(crate) fn pacing_status(&mut self, status: &Status) {
        let mpos = [status.x, status.y, status.z];
        self.stats.moving = self.stats.mpos.is_some_and(|before| before != mpos);
        self.stats.mpos = Some(mpos);
    }

    pub(crate) fn pacing_position_error(&mut self, error: &PositionError) {
        if !self.stats.in_job {
            return;
        }
        let peaks = error.peaks.filter(|_| self.peak_error);
        if let Some(peaks) = peaks {
            self.stats.tallies().for_each(|tally| tally.errors.ahead(peaks.ahead()));
        }
        if !self.stats.moving {
            return;
        }
        let mm = error.left.abs().max(error.right.abs());
        let max_mm = peaks.map_or(mm, |peaks| peaks.max());
        self.stats.tallies().for_each(|tally| tally.errors.add(mm, max_mm));
    }

    /// For a client that missed the pushes.
    pub(crate) fn pacing_shown(&self) -> Effect {
        broadcast(&*self.stats.shown)
    }
}
