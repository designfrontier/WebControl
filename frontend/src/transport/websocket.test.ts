import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  connectWebSocket,
  createWebSocketTransport,
  decodeFrame,
  encodeFrame,
  reconnectDelay,
  webSocketUrl,
} from "./websocket";
import type { WebSocketLike } from "./websocket";

describe("plain WebSocket frames", () => {
  it("wraps one socket event per frame as {ns, event, data}", () => {
    const payload = { data: { command: "move", arg: "up", arg1: 10 } };
    const text = encodeFrame("/MaslowCNC", "action", payload);
    expect(JSON.parse(text)).toEqual({ ns: "/MaslowCNC", event: "action", data: payload });
  });

  it("carries a server message envelope unchanged, JSON text and all", () => {
    const envelope = { command: "healthMessage", data: '{"cpuUsage":12.5,"bufferSize":127}', dataFormat: "json" };
    expect(decodeFrame(encodeFrame("/MaslowCNC", "message", envelope))).toEqual({
      ns: "/MaslowCNC",
      event: "message",
      data: envelope,
    });
  });

  it.each([["not json"], ['{"event":"message"}'], ['["/MaslowCNC","message"]']])("rejects %s", (text) => {
    expect(decodeFrame(text)).toBeUndefined();
  });

  it("connects to /ws on the page's own host, naming the namespace", () => {
    expect(webSocketUrl("/MaslowCNC", { protocol: "http:", host: "pi.local:5000" })).toBe(
      "ws://pi.local:5000/ws?ns=%2FMaslowCNC",
    );
    expect(webSocketUrl("/MaslowCNCLogs", { protocol: "https:", host: "cnc" })).toBe(
      "wss://cnc/ws?ns=%2FMaslowCNCLogs",
    );
  });
});

describe("reconnect delays", () => {
  it("start at 1 s and double to a 5 s cap", () => {
    const middle = () => 0.5;
    expect([0, 1, 2, 3, 4, 10].map((attempt) => reconnectDelay(attempt, middle))).toEqual([
      1000, 2000, 4000, 5000, 5000, 5000,
    ]);
  });

  it("are randomised by ±50%", () => {
    expect(reconnectDelay(0, () => 0)).toBe(500);
    expect(reconnectDelay(0, () => 1)).toBe(1500);
    expect(reconnectDelay(5, () => 1)).toBe(7500);
  });
});

class FakeWebSocket implements WebSocketLike {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly sent: string[] = [];
  closedByClient = false;
  readonly url: string;

  constructor(url: string) {
    this.url = url;
  }

  send(data: string) {
    this.sent.push(data);
  }
  close() {
    this.closedByClient = true;
  }
  serverOpen() {
    this.onopen?.({});
  }
  serverFrame(event: string, data: unknown, ns = "/MaslowCNC") {
    this.serverRaw(encodeFrame(ns, event, data));
  }
  serverRaw(data: unknown) {
    this.onmessage?.(new MessageEvent("message", { data }));
  }
  serverHandshake(sid = "sid-1") {
    this.serverOpen();
    this.serverFrame("connect", { sid });
  }
  serverClose() {
    this.onerror?.({});
    this.onclose?.(new CloseEvent("close"));
  }
}

const setup = (random = () => 0.5) => {
  const sockets: FakeWebSocket[] = [];
  const transport = createWebSocketTransport("/MaslowCNC", {
    open: (url) => {
      const socket = new FakeWebSocket(url);
      sockets.push(socket);
      return socket;
    },
    location: { protocol: "http:", host: "pi.local:5000" },
    random,
  });
  const events: [string, unknown][] = [];
  ["connect", "disconnect", "connect_error", "message", "hostAddress"].forEach((name) =>
    transport.on(name, (...args: never[]) => events.push([name, args[0]])),
  );
  const latest = () => sockets[sockets.length - 1]!;
  return { transport, sockets, events, latest };
};

describe("the WebSocket transport", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("opens one socket for its namespace at once and is connecting until the handshake", () => {
    const { transport, sockets, events, latest } = setup();
    expect(sockets.map((s) => s.url)).toEqual(["ws://pi.local:5000/ws?ns=%2FMaslowCNC"]);
    latest().serverOpen();
    expect(transport.connected).toBe(false);
    expect(transport.id).toBeUndefined();
    expect(events).toEqual([]);
  });

  it("sends nothing and drops server frames before the handshake", () => {
    const { transport, events, latest } = setup();
    latest().serverOpen();
    transport.emit("action", { data: { command: "home", arg: "", arg1: "" } });
    latest().serverFrame("message", { command: "hostAddress" });
    expect(latest().sent).toEqual([]);
    expect(events).toEqual([]);
  });

  it("is connected once the server names the socket", () => {
    const { transport, events, latest } = setup();
    latest().serverHandshake("abc");
    expect(transport.connected).toBe(true);
    expect(transport.id).toBe("abc");
    expect(events).toEqual([["connect", undefined]]);
  });

  it("sends each emit as one frame of its namespace", () => {
    const { transport, latest } = setup();
    latest().serverHandshake();
    const payload = { data: { section: "Computed Settings", setting: "units" } };
    transport.emit("settingRequest", payload);
    expect(latest().sent.map((text) => JSON.parse(text))).toEqual([
      { ns: "/MaslowCNC", event: "settingRequest", data: payload },
    ]);
  });

  it("hands each server event's data to its listeners", () => {
    const { events, latest } = setup();
    latest().serverHandshake();
    const envelope = { command: "hostAddress", data: '{"hostAddress":"h"}', dataFormat: "json" };
    latest().serverFrame("message", envelope);
    latest().serverFrame("hostAddress", 1);
    expect(events.slice(1)).toEqual([
      ["message", envelope],
      ["hostAddress", 1],
    ]);
  });

  it("ignores frames for another namespace, garbage, binary data and a repeated handshake", () => {
    const { transport, events, latest } = setup();
    latest().serverHandshake("first");
    latest().serverFrame("message", {}, "/MaslowCNCLogs");
    latest().serverRaw("not json");
    latest().serverRaw(new ArrayBuffer(2));
    latest().serverFrame("connect", { sid: "second" });
    expect(events).toEqual([["connect", undefined]]);
    expect(transport.id).toBe("first");
  });

  it("ignores a handshake without a sid", () => {
    const { transport, latest } = setup();
    latest().serverOpen();
    latest().serverFrame("connect", {});
    expect(transport.connected).toBe(false);
  });

  it("reports a close it did not ask for as a disconnect and reconnects after a second", () => {
    const { transport, sockets, events, latest } = setup();
    latest().serverHandshake("one");
    latest().serverClose();
    expect(transport.connected).toBe(false);
    expect(transport.id).toBeUndefined();
    expect(events.at(-1)).toEqual(["disconnect", "transport close"]);
    vi.advanceTimersByTime(999);
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(2);
    latest().serverHandshake("two");
    expect(transport.id).toBe("two");
  });

  it("drops emits while disconnected rather than replaying them later", () => {
    const { transport, latest } = setup();
    latest().serverHandshake();
    latest().serverClose();
    transport.emit("action", { data: { command: "move", arg: "up", arg1: 10 } });
    vi.advanceTimersByTime(1000);
    latest().serverHandshake();
    expect(latest().sent).toEqual([]);
  });

  it("reports a failed open as connect_error and backs off, doubling to 5 s, until one succeeds", () => {
    const { sockets, events, latest } = setup();
    const delays = [1000, 2000, 4000, 5000, 5000];
    delays.forEach((delay, attempt) => {
      latest().serverClose();
      expect(events.at(-1)?.[0]).toBe("connect_error");
      vi.advanceTimersByTime(delay - 1);
      expect(sockets).toHaveLength(attempt + 1);
      vi.advanceTimersByTime(1);
      expect(sockets).toHaveLength(attempt + 2);
    });
    latest().serverHandshake();
    latest().serverClose();
    vi.advanceTimersByTime(1000);
    expect(sockets).toHaveLength(delays.length + 2);
  });

  it("stops for good on disconnect(), without raising events", () => {
    const { transport, sockets, events, latest } = setup();
    latest().serverHandshake();
    transport.disconnect();
    expect(latest().closedByClient).toBe(true);
    expect(transport.connected).toBe(false);
    latest().serverClose();
    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(1);
    expect(events).toEqual([["connect", undefined]]);
  });

  it("cancels a pending reconnect on disconnect() and opens again on connect()", () => {
    const { transport, sockets, latest } = setup();
    latest().serverClose();
    transport.disconnect();
    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(1);
    transport.connect();
    expect(sockets).toHaveLength(2);
    transport.connect();
    expect(sockets).toHaveLength(2);
  });

  it("opens at once when asked to connect during a backoff", () => {
    const { transport, sockets, latest } = setup();
    latest().serverClose();
    transport.connect();
    expect(sockets).toHaveLength(2);
    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(2);
  });

  it("uses the browser's WebSocket and location by default", () => {
    const created: string[] = [];
    vi.stubGlobal(
      "WebSocket",
      class extends FakeWebSocket {
        constructor(url: string) {
          super(url);
          created.push(url);
        }
      },
    );
    const transport = connectWebSocket("/MaslowCNCLogs");
    expect(created).toEqual([webSocketUrl("/MaslowCNCLogs", window.location)]);
    transport.disconnect();
  });
});
