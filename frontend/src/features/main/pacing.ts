import { useEffect, useRef, useState } from "react";
import { ApiError, getJson, sendJson } from "../../api/http";
import type { Fetch } from "../../api/http";
import { useFetch, useMachine, useServerMessage } from "../../app/context";
import { PACING_GAP_MAX_MS, pacingStatsSchema } from "../../protocol";
import type { PacingMode, PacingSegment, PacingStats } from "../../protocol";

export interface Pacing {
  mode: PacingMode;
  gapMs: number;
}

export const PRESETS = [
  { id: "v094-50", label: "v0.94 50 ms", mode: "v0.94", gapMs: 50 },
  { id: "min-50", label: "Min 50 ms", mode: "minimum gap", gapMs: 50 },
  { id: "min-25", label: "Min 25 ms", mode: "minimum gap", gapMs: 25 },
  { id: "fastest", label: "Min 0 ms", mode: "minimum gap", gapMs: 0 },
] as const satisfies readonly (Pacing & { id: string; label: string })[];

export const CUSTOM = "custom";

export const presetFor = ({ mode, gapMs }: Pacing) =>
  PRESETS.find((preset) => preset.mode === mode && preset.gapMs === gapMs)?.id ?? CUSTOM;

export const pacingLabel = ({ mode, gapMs }: Pacing) => `${mode === "v0.94" ? "v0.94" : "min gap"} ${gapMs} ms`;

export const validGap = (text: string) => {
  const gap = Number(text);
  return text.trim() !== "" && Number.isInteger(gap) && gap >= 0 && gap <= PACING_GAP_MAX_MS ? gap : null;
};

export const fixed = (value: number | null, digits: number) => (value === null ? "–" : value.toFixed(digits));

const lostShort = ({ lost, lostPercent }: PacingSegment) =>
  lostPercent === null ? `Lost ${lost}` : `Lost ${lost} (${fixed(lostPercent, 2)}%)`;

/** "507 lines/min", "Lost 9 (0.39%)" and the rest, as the run panel shows a segment. */
export const describe = (segment: PacingSegment) => ({
  rate: segment.linesPerMinute === null ? "No lines yet" : `${fixed(segment.linesPerMinute, 0)} lines/min`,
  lost: lostShort(segment),
  detail: [
    `${segment.lines} lines sent`,
    lostShort(segment),
    ...(segment.lostTwice > 0 ? [`${segment.lostTwice} lost twice`] : []),
    `${fixed(segment.secondsPerLine, 3)} s/line`,
    `${segment.recoveryS.toFixed(1)} s recovering`,
    pacingLabel(segment),
  ].join(" · "),
});

export const savePacing = (fetchImpl: Fetch, { mode, gapMs }: Pacing) =>
  sendJson(
    fetchImpl,
    "PUT",
    "/run/pacing",
    { values: { linePacingMode: mode, linePacingGapMs: gapMs } },
    pacingStatsSchema,
  );

/** The server's per-setting message, or a generic one. */
export const refusal = (error: unknown) => {
  const errors =
    error instanceof ApiError ? (error.body as { errors?: Record<string, string> } | undefined) : undefined;
  const first = errors?.errors && Object.values(errors.errors)[0];
  return first ?? "Could not change the pacing.";
};

/** GET /api/run/pacing on every (re)connect, then the pacingStats pushes. */
export const usePacingStats = () => {
  const fetch = useFetch();
  const socket = useMachine((s) => s.socket);
  const [stats, setStats] = useState<PacingStats | null>(null);
  const updates = useRef(0);
  const update = (next: PacingStats) => {
    updates.current += 1;
    setStats(next);
  };
  useServerMessage("pacingStats", update);
  useEffect(() => {
    if (socket !== "connected") return;
    let live = true;
    const issued = updates.current;
    getJson(fetch, "/run/pacing", pacingStatsSchema).then(
      (loaded) => {
        // A push or save since the request is newer than this answer.
        if (live && updates.current === issued) setStats(loaded);
      },
      // The next push brings the same numbers.
      () => {},
    );
    return () => {
      live = false;
    };
  }, [socket, fetch]);
  return [stats, update] as const;
};
