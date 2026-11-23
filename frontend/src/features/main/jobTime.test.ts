import { describe, expect, it } from "vitest";
import type { PacingSegment, PacingStats } from "../../protocol";
import { duration, jobTime } from "./jobTime";

const job = (over: Partial<PacingSegment> = {}): PacingSegment => ({
  mode: "v0.94",
  gapMs: 50,
  lines: 100,
  lost: 0,
  lostTwice: 0,
  elapsedS: 3000,
  recoveryS: 0,
  linesPerMinute: 2,
  secondsPerLine: 30,
  lostPercent: 0,
  medianShortGapMs: null,
  endedBy: null,
  wallS: 3725,
  remainingS: 720,
  ...over,
});

const stats = (running: PacingSegment | null): PacingStats => ({
  mode: "v0.94",
  gapMs: 50,
  job: running,
  segment: null,
  history: [],
  estimateS: 4000,
});

describe("duration", () => {
  it("gives seconds under a minute, then minutes, then hours and minutes", () => {
    expect([0, 45, 59.4, 59.6, 90, 3569, 3600, 3725, 36000].map(duration)).toEqual([
      "0s",
      "45s",
      "59s",
      "1m",
      "2m",
      "59m",
      "1h 00m",
      "1h 02m",
      "10h 00m",
    ]);
  });
});

describe("jobTime", () => {
  it("gives the time left first, then wall and cutting time, while a job runs", () => {
    expect(jobTime(stats(job()))).toBe("~12m left · 1h 02m · cutting 50m");
  });

  it("drops the time left once the job ends or when there is no estimate", () => {
    expect(jobTime(stats(job({ endedBy: "job end" })))).toBe("1h 02m · cutting 50m");
    expect(jobTime(stats(job({ remainingS: null })))).toBe("1h 02m · cutting 50m");
  });

  it("is null before any job, or from a backend without the timer", () => {
    expect(jobTime(null)).toBeNull();
    expect(jobTime(stats(null))).toBeNull();
    expect(jobTime(stats(job({ wallS: undefined })))).toBeNull();
  });
});
