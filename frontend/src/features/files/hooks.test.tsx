import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useSubmit } from "./hooks";

describe("useSubmit", () => {
  afterEach(() => vi.useRealTimers());

  it("clears busy and reports the failure when the task throws before awaiting", async () => {
    const { result } = renderHook(() => useSubmit());
    await act(() =>
      result.current.run(() => {
        throw new Error("boom");
      }, "Could not do it."),
    );
    expect(result.current.busy).toBe(false);
    expect(result.current.error).toBe("Could not do it.");
  });

  it("gives up after the timeout with a generic message, aborting the task's signal", async () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useSubmit({ timeoutMs: 1000 }));
    let signal: AbortSignal | undefined;
    let running: Promise<void> = Promise.resolve();
    act(() => {
      running = result.current.run((given) => {
        signal = given;
        return new Promise(() => {});
      }, "Could not do it.");
    });
    expect(result.current.busy).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
      await running;
    });
    expect(signal?.aborted).toBe(true);
    expect(result.current.busy).toBe(false);
    expect(result.current.error).toBe("The server did not answer in time.");
  });

  it("aborts the task when the component unmounts mid-load, so its follow-up never runs", async () => {
    const { result, unmount } = renderHook(() => useSubmit());
    const followUp = vi.fn();
    let answer = () => {};
    let running: Promise<void> = Promise.resolve();
    act(() => {
      running = result.current.run(async (signal) => {
        await new Promise<void>((resolve) => (answer = resolve));
        signal.throwIfAborted();
        followUp();
      }, "Could not do it.");
    });

    unmount();
    answer();
    await running;
    expect(followUp).not.toHaveBeenCalled();
  });
});
