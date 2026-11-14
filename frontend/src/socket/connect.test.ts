import { expect, it, vi } from "vitest";

const io = vi.hoisted(() => vi.fn(() => ({ id: undefined, connected: false })));
vi.mock("socket.io-client", () => ({ io }));

it("connects to the /MaslowCNC namespace on the page's own server", async () => {
  const { connectSocket } = await import("./connect");
  connectSocket();
  expect(io).toHaveBeenCalledWith("/MaslowCNC", expect.objectContaining({ path: "/socket.io" }));
});
