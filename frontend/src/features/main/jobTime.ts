import type { PacingStats } from "../../protocol";

/** "45s", "12m", "1h 05m": short enough for the readouts on a phone. */
export const duration = (seconds: number) => {
  const minutes = Math.round(seconds / 60);
  if (seconds < 59.5) return `${Math.round(seconds)}s`;
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
};

/** While a job runs its time left, then the running or last job's wall and cutting time; null before any job. */
export const jobTime = (pacing: PacingStats | null) => {
  const job = pacing?.job;
  if (!job || job.wallS === undefined) return null;
  const left = job.endedBy === null && job.remainingS != null ? [`~${duration(job.remainingS)} left`] : [];
  return [...left, duration(job.wallS), `cutting ${duration(job.elapsedS)}`].join(" · ");
};
