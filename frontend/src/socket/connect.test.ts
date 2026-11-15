import { expect, it, vi } from "vitest";

const io = vi.hoisted(() => vi.fn(() => ({ id: undefined, connected: false })));
vi.mock("socket.io-client", () => ({ io }));

it("connects to the /MaslowCNC namespace on the page's own server", async () => {
  const { connectSocket } = await import("./connect");
  connectSocket();
  expect(io).toHaveBeenCalledWith("/MaslowCNC", { path: "/socket.io", reconnectionDelayMax: 5000 });
});

it("opens the log stream on a connection of its own", async () => {
  const { connectLogSocket } = await import("./connect");
  connectLogSocket();
  expect(io).toHaveBeenLastCalledWith("/MaslowCNCLogs", {
    path: "/socket.io",
    reconnectionDelayMax: 5000,
    forceNew: true,
  });
});
