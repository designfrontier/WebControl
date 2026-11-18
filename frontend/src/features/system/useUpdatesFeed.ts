import { useCallback, useEffect, useState } from "react";
import { useFetch } from "../../app/context";
import type { Updates } from "../../protocol";
import { getUpdates } from "./api";
import { POLL_MS } from "./updateProgress";
import { useLoad } from "./useLoad";

/**
 * GET /api/system/updates, polled while an update runs. A failed poll is WebControl restarting: it keeps polling
 * and says it is reconnecting, so the final result shows once the new version answers.
 */
export const useUpdatesFeed = () => {
  const fetchImpl = useFetch();
  const loaded = useLoad(getUpdates);
  const [fresh, setFresh] = useState<Updates | null>(null);
  const [reconnecting, setReconnecting] = useState(false);
  const [watched, setWatched] = useState(false);
  const data = fresh ?? (loaded.status === "ready" ? loaded.data : null);
  const running = data?.progress?.running ?? false;

  useEffect(() => {
    if (!running) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = () => {
      timer = setTimeout(() => {
        setWatched(true);
        getUpdates(fetchImpl)
          .then(
            (next) => live && (setFresh(next), setReconnecting(false)),
            () => live && setReconnecting(true),
          )
          .finally(() => live && poll());
      }, POLL_MS);
    };
    poll();
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [running, fetchImpl]);

  const reload = useCallback(() => {
    setFresh(null);
    loaded.reload();
  }, [loaded]);
  return { loaded, data, reconnecting, watched, show: setFresh, reload };
};
