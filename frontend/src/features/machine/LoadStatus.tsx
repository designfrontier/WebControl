import type { Load } from "./useLoad";

interface Props {
  load: Load<unknown>;
  what: string;
  onRetry: () => void;
}

/** Shows progress or the failure of a Load; renders nothing once it is ready. */
export const LoadStatus = ({ load, what, onRetry }: Props) =>
  load.status === "loading" ? (
    <p className="muted" role="status">
      Loading {what}…
    </p>
  ) : load.status === "error" ? (
    <div className="notice" data-tone="bad" role="alert">
      <p>
        Could not load {what}: {load.error.message}
      </p>
      <button type="button" onClick={onRetry}>
        Retry
      </button>
    </div>
  ) : null;
