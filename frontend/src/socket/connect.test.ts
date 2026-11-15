import { afterEach, expect, it, vi } from "vitest";

const connectWebSocket = vi.hoisted(() => vi.fn(() => ({ id: undefined, connected: false })));
vi.mock("../transport/websocket", () => ({ connectWebSocket }));

afterEach(() => connectWebSocket.mockClear());

it("connects to the /MaslowCNC namespace on the page's own server", async () => {
  const { connectSocket } = await import("./connect");
  connectSocket();
  expect(connectWebSocket).toHaveBeenCalledWith("/MaslowCNC");
});

it("opens the log stream on a connection of its own", async () => {
  const { connectLogSocket } = await import("./connect");
  connectLogSocket();
  expect(connectWebSocket).toHaveBeenLastCalledWith("/MaslowCNCLogs");
});
