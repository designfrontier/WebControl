import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createFakeSocket } from "../../test/fakeSocket";
import type { FakeSocket } from "../../test/fakeSocket";
import { renderWithProviders } from "../../test/render";
import { LOG_LINE_LIMIT, initialLogView, logReducer } from "./logStream";
import { LogsPanel } from "./LogsPanel";

const connectWebSocket = vi.hoisted(() => vi.fn(() => ({ id: undefined, connected: false })));
vi.mock("../../transport/websocket", () => ({ connectWebSocket }));

/** LogStreamer queues each entry with its trailing newline. */
const line = (log: "log" | "alog", data: string, state = false) => ({ log, data: `${data}\n`, state });

/** StrictMode mounts twice, so each connect gets a fresh socket like the real one; tests drive the last. */
const renderLogs = ({ connected = false } = {}) => {
  const sockets: FakeSocket[] = [];
  const connect = vi.fn(() => sockets[sockets.push(createFakeSocket({ connected })) - 1]);
  const result = renderWithProviders(<LogsPanel connect={connect} />);
  return { ...result, logSocket: sockets.at(-1)!, sockets };
};

const loggerState = () => within(screen.getByRole("region", { name: "Logs" })).getByRole("status");
const pane = (name: string) => within(screen.getByRole("region", { name }));
const lines = (name: string) => pane(name).getByLabelText(`${name} lines`).textContent;

describe("LogsPanel", () => {
  it("opens its own log socket and reports the logger state", () => {
    const { logSocket } = renderLogs();
    expect(loggerState()).toHaveTextContent("Not connected to the log stream");

    logSocket.serverConnect();
    expect(loggerState()).toHaveTextContent("Waiting for status…");

    logSocket.serverRaw({ log: "state", state: false, dataFormat: "text" });
    expect(loggerState()).toHaveTextContent("Logging active");
    expect(loggerState()).toHaveAttribute("data-tone", "ok");

    logSocket.serverRaw({ log: "state", state: true, dataFormat: "text" });
    expect(loggerState()).toHaveTextContent("Logging suspended");

    logSocket.serverDisconnect();
    expect(loggerState()).toHaveAttribute("data-tone", "bad");
  });

  it("starts connected when the socket already is", () => {
    renderLogs({ connected: true });
    expect(loggerState()).toHaveTextContent("Waiting for status…");
  });

  it("appends lines to the matching pane and ignores malformed messages", () => {
    const { logSocket } = renderLogs();
    logSocket.serverRaw(line("log", "12.00: Connected"));
    logSocket.serverRaw(line("alog", "12.01: G0 X1"));
    logSocket.serverRaw(line("log", "12.02: ok"));
    logSocket.serverRaw({ log: "log" });

    expect(lines("Log")).toBe("12.00: Connected\n12.02: ok");
    expect(lines("Abridged log")).toBe("12.01: G0 X1");
    expect(pane("Log").getByText("2 lines shown (newest 1000 kept)")).toBeInTheDocument();
    expect(pane("Abridged log").getByText("1 line shown (newest 1000 kept)")).toBeInTheDocument();
  });

  it("stops streaming a pane while its switch is off, and clears the view", async () => {
    const { logSocket, user } = renderLogs({ connected: true });
    logSocket.serverRaw(line("log", "kept"));
    await user.click(pane("Log").getByRole("checkbox", { name: "Stream new lines" }));
    logSocket.serverRaw(line("log", "skipped", true));
    expect(lines("Log")).toBe("kept");
    expect(loggerState()).toHaveTextContent("Logging suspended");

    await user.click(pane("Log").getByRole("checkbox", { name: "Stream new lines" }));
    logSocket.serverRaw(line("log", "back"));
    expect(lines("Log")).toBe("kept\nback");

    await user.click(pane("Log").getByRole("button", { name: "Clear view" }));
    expect(lines("Log")).toBe("");
  });

  it("strips the server's CRLF too", () => {
    const { logSocket } = renderLogs();
    logSocket.serverRaw({ log: "log", data: "a\r\n", state: false });
    logSocket.serverRaw({ log: "log", data: "b\r\n", state: false });
    expect(lines("Log")).toBe("a\nb");
  });

  it("follows new lines only while the pane is scrolled to the bottom", () => {
    const { logSocket } = renderLogs();
    const output = pane("Log").getByLabelText("Log lines");
    Object.defineProperties(output, {
      scrollHeight: { configurable: true, value: 500 },
      clientHeight: { configurable: true, value: 100 },
    });
    logSocket.serverRaw(line("log", "first"));
    expect(output.scrollTop).toBe(500);

    output.scrollTop = 200;
    fireEvent.scroll(output);
    logSocket.serverRaw(line("log", "second"));
    expect(output.scrollTop).toBe(200);

    output.scrollTop = 400;
    fireEvent.scroll(output);
    logSocket.serverRaw(line("log", "third"));
    expect(output.scrollTop).toBe(500);
  });

  it("disconnects when it unmounts", () => {
    const { sockets, unmount } = renderLogs({ connected: true });
    unmount();
    expect(sockets.map((socket) => socket.connected)).toEqual([false, false]);
  });

  it("connects to the /MaslowCNCLogs namespace by default", async () => {
    const { connectLogSocket } = await import("./logStream");
    connectLogSocket();
    expect(connectWebSocket).toHaveBeenCalledWith("/MaslowCNCLogs");
  });
});

describe("logReducer", () => {
  it("keeps only the newest lines", () => {
    const view = Array.from({ length: LOG_LINE_LIMIT + 3 }, (_, i) => i).reduce(
      (state, i) => logReducer(state, { type: "message", message: line("alog", `l${i}`) }),
      initialLogView,
    );
    expect(view.lines.alog).toHaveLength(LOG_LINE_LIMIT);
    expect(view.lines.alog[0]).toBe("l3");
  });
});
