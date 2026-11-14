import { useEffect, useState } from "react";

/** Seconds left on a one-second countdown that `start` (re)starts; null when idle. */
export const useCountdown = () => {
  const [remaining, setRemaining] = useState<number | null>(null);
  const running = remaining !== null && remaining > 0;

  useEffect(() => {
    if (!running) return;
    const timer = setInterval(() => setRemaining((left) => (left === null ? null : Math.max(left - 1, 0))), 1000);
    return () => clearInterval(timer);
  }, [running]);

  return { remaining, start: (seconds: number) => setRemaining(seconds), reset: () => setRemaining(null) };
};
