import { describe, expect, it, vi } from "vitest";
import { createClient } from "../socket/client";
import { startupState } from "../test/fakeFetch";
import { createFakeSocket } from "../test/fakeSocket";
import { bindMachineStore } from "./bind";
import { createMachineStore } from "./machine";

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const setup = (loadStartup = vi.fn(() => Promise.resolve(startupState({ units: "INCHES" })))) => {
  const socket = createFakeSocket();
  const store = createMachineStore();
  const unbind = bindMachineStore(store, createClient(socket), loadStartup);
  return { socket, store, unbind, loadStartup };
};

describe("bindMachineStore", () => {
  it("syncs on connect: nudges a position report and loads startup state", async () => {
    const { socket, store, loadStartup } = setup();
    expect(store.getState().socket).toBe("connecting");
    expect(loadStartup).not.toHaveBeenCalled();

    socket.serverConnect();
    await flush();

    expect(store.getState()).toMatchObject({ socket: "connected", units: "INCHES" });
    expect(socket.emittedEvents("settingRequest")).toEqual([
      { data: { section: "Computed Settings", setting: "units" } },
    ]);
  });

  it("syncs straight away when the socket is already connected", async () => {
    const socket = createFakeSocket({ connected: true });
    const store = createMachineStore();
    bindMachineStore(store, createClient(socket), () => Promise.resolve(startupState({ unitsZ: "INCHES" })));
    await flush();
    expect(store.getState()).toMatchObject({ socket: "connected", unitsZ: "INCHES" });
  });

  it("resyncs after a reconnect", async () => {
    const { socket, loadStartup } = setup();
    socket.serverConnect();
    socket.serverDisconnect();
    socket.serverConnect();
    await flush();
    expect(loadStartup).toHaveBeenCalledTimes(2);
  });

  it("keeps the newest startup snapshot when an older one resolves later", async () => {
    const pending: ((state: ReturnType<typeof startupState>) => void)[] = [];
    const { socket, store } = setup(vi.fn(() => new Promise((resolve) => pending.push(resolve))));
    socket.serverConnect();
    socket.serverDisconnect();
    socket.serverConnect();

    pending[1](startupState({ units: "MM" }));
    await flush();
    pending[0](startupState({ units: "INCHES" }));
    await flush();

    expect(store.getState().units).toBe("MM");
  });

  it("feeds store commands into the store", () => {
    const { socket, store } = setup();
    socket.serverConnect();
    socket.serverMessage("alarm", { message: "Alarm: Sled Not Keeping Up" });
    expect(store.getState().alarm).toBe("Alarm: Sled Not Keeping Up");
  });

  it("keeps the link state the server pushed before the store was bound", async () => {
    const socket = createFakeSocket();
    const client = createClient(socket);
    socket.serverConnect("me");
    socket.serverMessage("hostAddress", { hostAddress: "10.0.0.5:5000" });
    socket.serverMessage("connectionStatus", { status: "connected", port: "/dev/ttyACM0", fakeServoStatus: false });

    const store = createMachineStore();
    bindMachineStore(store, client, () => Promise.resolve(startupState()));
    await flush();

    expect(store.getState().controller).toEqual({ status: "connected", port: "/dev/ttyACM0", fakeServo: false });
  });

  it("warns and keeps going when /api/state fails", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const { socket, store } = setup(vi.fn(() => Promise.reject(new Error("503"))));
    socket.serverConnect();
    await flush();
    expect(warn).toHaveBeenCalledWith("Could not load /api/state", expect.any(Error));
    expect(store.getState().units).toBeNull();
  });

  it("stops updating the store once unbound, including late startup responses", async () => {
    const { socket, store, unbind } = setup();
    socket.serverConnect();
    unbind();
    await flush();
    socket.serverMessage("alarm", { message: "late" });
    expect(store.getState()).toMatchObject({ alarm: null, units: null });
  });
});
