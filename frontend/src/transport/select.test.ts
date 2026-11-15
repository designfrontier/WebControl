import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeFetch, jsonResponse, startupState } from "../test/fakeFetch";
import type { Transport } from "./types";

const io = vi.hoisted(() => vi.fn(() => ({ id: undefined, connected: false, on: vi.fn() })));
vi.mock("socket.io-client", () => ({ io }));

const { connectTransport, probeBackend, selectTransport, watchBackend } = await import("./select");

const stateWith = (backend?: "python" | "rust") => fakeFetch({ "GET /api/state": startupState({ backend }) });

describe("probeBackend", () => {
  it.each([["rust"], ["python"]] as const)("reads %s from /api/state", async (backend) => {
    await expect(probeBackend(stateWith(backend))).resolves.toBe(backend);
  });

  it("takes a Python build that predates the field for Python", async () => {
    await expect(probeBackend(stateWith(undefined))).resolves.toBe("python");
  });

  it("is undefined when the server cannot be reached or answers nonsense", async () => {
    await expect(probeBackend(fakeFetch({ "GET /api/state": () => jsonResponse({}, 502) }))).resolves.toBeUndefined();
    await expect(probeBackend(vi.fn(() => Promise.reject(new Error("offline"))))).resolves.toBeUndefined();
    await expect(probeBackend(fakeFetch({ "GET /api/state": { backend: "perl" } }))).resolves.toBeUndefined();
  });
});

describe("selectTransport", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    io.mockClear();
  });

  it("uses plain WebSockets for the Rust backend", async () => {
    const opened: string[] = [];
    vi.stubGlobal(
      "WebSocket",
      class {
        onmessage = null;
        onclose = null;
        constructor(url: string) {
          opened.push(url);
        }
        send() {}
        close() {}
      },
    );
    await expect(selectTransport(stateWith("rust"))).resolves.toBe("rust");
    connectTransport("/MaslowCNC").disconnect();
    expect(opened).toHaveLength(1);
    expect(io).not.toHaveBeenCalled();
  });

  it.each([["python"], [undefined]] as const)("uses socket.io when the backend is %s", async (backend) => {
    await selectTransport(stateWith(backend));
    connectTransport("/MaslowCNC", { separate: true });
    expect(io).toHaveBeenCalledWith("/MaslowCNC", expect.objectContaining({ forceNew: true }));
  });

  it("falls back to socket.io when /api/state cannot be read", async () => {
    await expect(selectTransport(vi.fn(() => Promise.reject(new Error("down"))))).resolves.toBeUndefined();
    connectTransport("/MaslowCNC");
    expect(io).toHaveBeenCalledTimes(1);
  });
});

const fakeTransport = () => {
  const handlers = new Map<string, () => void>();
  const transport = {
    on: (event: string, handler: () => void) => handlers.set(event, handler),
  } as unknown as Transport;
  const fire = (event: string) => handlers.get(event)?.();
  return { transport, fire };
};

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("watchBackend", () => {
  it("reloads after three failed connects in a row when the backend changed", async () => {
    const { transport, fire } = fakeTransport();
    const reload = vi.fn();
    const fetch = stateWith("rust");
    watchBackend(transport, "python", { fetch, reload });
    fire("connect_error");
    fire("connect_error");
    await settle();
    expect(fetch).not.toHaveBeenCalled();
    fire("connect_error");
    await settle();
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("keeps the page when the same backend answers or none does", async () => {
    const { transport, fire } = fakeTransport();
    const reload = vi.fn();
    watchBackend(transport, "rust", { fetch: stateWith("rust"), reload });
    [1, 2, 3].forEach(() => fire("connect_error"));
    await settle();
    const down = fakeTransport();
    watchBackend(down.transport, "rust", { fetch: vi.fn(() => Promise.reject(new Error("down"))), reload });
    [1, 2, 3].forEach(() => down.fire("connect_error"));
    await settle();
    expect(reload).not.toHaveBeenCalled();
  });

  it("counts only failures in a row", async () => {
    const { transport, fire } = fakeTransport();
    const fetch = stateWith("rust");
    watchBackend(transport, "python", { fetch, reload: vi.fn() });
    fire("connect_error");
    fire("connect_error");
    fire("connect");
    fire("connect_error");
    fire("connect_error");
    await settle();
    expect(fetch).not.toHaveBeenCalled();
  });

  it("treats an unknown starting backend as Python", async () => {
    const { transport, fire } = fakeTransport();
    const reload = vi.fn();
    watchBackend(transport, undefined, { fetch: stateWith("python"), reload });
    [1, 2, 3].forEach(() => fire("connect_error"));
    await settle();
    expect(reload).not.toHaveBeenCalled();
  });
});
