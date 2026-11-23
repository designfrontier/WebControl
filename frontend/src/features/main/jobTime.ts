import type { PacingStats } from "../../protocol";

/** "45s", "12m", "1h 05m": short enough for the readouts on a phone. */
export const duration = (seconds: number) => {
  const minutes = Math.round(seconds / 60);
  if (seconds < 59.5) return `${Math.round(seconds)}s`;
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
};

/** The time left while a job runs, else the loaded file's estimate; then the job's wall and cutting time. */
export const jobTime = (pacing: PacingStats | null) => {
  const job = pacing?.job;
  const wall = job?.wallS;
  const running = job?.endedBy === null;
  const ahead = running ? job.remainingS : pacing?.estimateS;
  const parts = [
    ahead == null ? null : running ? `~${duration(ahead)} left` : `est. ${duration(ahead)}`,
    ...(job && wall !== undefined ? [duration(wall), `cutting ${duration(job.elapsedS)}`] : []),
  ].filter((part) => part !== null);
  return parts.length > 0 ? parts.join(" · ") : null;
};
