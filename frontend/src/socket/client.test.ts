import { describe, expect, it, vi } from "vitest";
import { createFakeSocket } from "../test/fakeSocket";
import { createClient } from "./client";

const setup = (connected = true) => {
  const socket = createFakeSocket({ connected, id: "me" });
  return { socket, client: createClient(socket) };
};

describe("createClient messages", () => {
  it("delivers validated, decoded data to subscribers of that command", () => {
    const { socket, client } = setup();
    const onHealth = vi.fn();
    const onOther = vi.fn();
    client.onMessage("healthMessage", onHealth);
    client.onMessage("alarm", onOther);

    socket.serverMessage("healthMessage", { cpuUsage: 12.5, bufferSize: 127 });

    expect(onHealth).toHaveBeenCalledWith({ cpuUsage: 12.5, bufferSize: 127 });
    expect(onOther).not.toHaveBeenCalled();
  });

  it("replays messages that arrived before anyone listened, once the first listener binds", async () => {
    const { socket, client } = setup();
    socket.serverMessage("healthMessage", { cpuUsage: 1, bufferSize: 0 });
    socket.serverMessage("alarm", { message: "Early alarm" });
    const onHealth = vi.fn();
    const onAlarm = vi.fn();
    client.onMessage("healthMessage", onHealth);
    client.onMessage("alarm", onAlarm);
    expect(onHealth).not.toHaveBeenCalled();

    await Promise.resolve();

    expect(onHealth).toHaveBeenCalledWith({ cpuUsage: 1, bufferSize: 0 });
    expect(onAlarm).toHaveBeenCalledWith({ message: "Early alarm" });
  });

  it("replays held messages before a newer one that beats the replay", () => {
    const { socket, client } = setup();
    socket.serverMessage("healthMessage", { cpuUsage: 1, bufferSize: 0 });
    const seen: number[] = [];
    client.onMessage("healthMessage", ({ cpuUsage }) => seen.push(cpuUsage));
    socket.serverMessage("healthMessage", { cpuUsage: 2, bufferSize: 0 });
    expect(seen).toEqual([1, 2]);
  });

  it("holds only the newest 256 messages", async () => {
    const { socket, client } = setup();
    Array.from({ length: 300 }, (_, i) => socket.serverMessage("healthMessage", { cpuUsage: i, bufferSize: 0 }));
    const seen: number[] = [];
    client.onMessage("healthMessage", ({ cpuUsage }) => seen.push(cpuUsage));
    await Promise.resolve();
    expect(seen).toEqual(Array.from({ length: 256 }, (_, i) => i + 44));
  });

  it("passes through data that is not JSON-encoded", () => {
    const { socket, client } = setup();
    const onSpinner = vi.fn();
    client.onMessage("showFPSpinner", onSpinner);
    socket.serverRaw({ command: "showFPSpinner", data: 2048, dataFormat: "int" });
    expect(onSpinner).toHaveBeenCalledWith(2048);
  });

  it.each([
    ["this socket", "me", true],
    ["every socket", "all", true],
    ["another socket", "someone-else", false],
  ])("filters messages addressed to %s", (_label, client, delivered) => {
    const { socket, client: wc } = setup();
    const onModal = vi.fn();
    wc.onMessage("activateModal", onModal);
    socket.serverMessage("activateModal", { title: "Help", message: "hi", client });
    expect(onModal).toHaveBeenCalledTimes(delivered ? 1 : 0);
  });

  it("drops invalid payloads with a warning", () => {
    const { socket, client } = setup();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const onHealth = vi.fn();
    client.onMessage("healthMessage", onHealth);

    socket.serverMessage("healthMessage", { cpuUsage: "lots" });
    socket.serverRaw("not an envelope");

    expect(onHealth).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledTimes(2);
  });

  it("ignores commands nobody subscribed to and commands without a schema", () => {
    const { socket, client } = setup();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    client.onMessage("alarm", vi.fn());
    socket.serverMessage("healthMessage", { cpuUsage: "not validated because unheard" });
    socket.serverMessage("somethingNew", {});
    expect(warn).not.toHaveBeenCalled();
  });

  it("stops delivering after unsubscribe", () => {
    const { socket, client } = setup();
    const onAlarm = vi.fn();
    const off = client.onMessage("alarm", onAlarm);
    off();
    socket.serverMessage("alarm", { message: "Position lost" });
    expect(onAlarm).not.toHaveBeenCalled();
  });
});

describe("createClient connection status", () => {
  it("starts as connecting or connected depending on the socket", () => {
    expect(setup(false).client.status).toBe("connecting");
    expect(setup(true).client.status).toBe("connected");
  });

  it("reports connect, disconnect and connect errors", () => {
    const { socket, client } = setup(false);
    const statuses: string[] = [];
    const off = client.onStatus((status) => statuses.push(status));

    socket.serverConnect();
    socket.serverDisconnect();
    off();
    socket.serverConnect();

    expect(statuses).toEqual(["connected", "disconnected"]);
    expect(client.status).toBe("connected");
  });

  it("treats a connect_error as disconnected", () => {
    const listeners: Record<string, () => void> = {};
    const socket = {
      id: undefined,
      connected: false,
      on: vi.fn(),
      emit: vi.fn(),
      connect: vi.fn(),
      disconnect: vi.fn(),
    };
    socket.on.mockImplementation((event: string, listener: () => void) => (listeners[event] = listener));
    const client = createClient(socket);
    listeners.connect_error();
    expect(client.status).toBe("disconnected");
  });
});

describe("createClient emits", () => {
  it("sends actions in the old { data: { command, arg, arg1 } } shape with blank defaults", () => {
    const { socket, client } = setup();
    client.sendAction("stopRun");
    client.sendAction("move", "upLeft", 10);
    expect(socket.actions()).toEqual([
      ["stopRun", "", ""],
      ["move", "upLeft", 10],
    ]);
  });

  it("requests and updates settings", () => {
    const { socket, client } = setup();
    client.requestSetting("Computed Settings", "units");
    client.updateSetting("toInches", "0.394");
    client.emit("checkForGCodeUpdate", { data: "Please" });
    expect(socket.emitted).toEqual([
      { event: "settingRequest", payload: { data: { section: "Computed Settings", setting: "units" } } },
      { event: "updateSetting", payload: { data: { setting: "toInches", value: "0.394" } } },
      { event: "checkForGCodeUpdate", payload: { data: "Please" } },
    ]);
  });

  it("close disconnects the socket", () => {
    const { socket, client } = setup();
    client.close();
    expect(socket.connected).toBe(false);
  });
});
