export interface Status {
  tone: "info" | "ok" | "error";
  message: string;
}

/** A polite live region, so screen readers hear each step's outcome. */
export const StatusLine = ({ status }: { status: Status | null }) => (
  <p className="status-line" role="status" aria-label="Calibration status" data-tone={status?.tone}>
    {status?.message}
  </p>
);
