import { describe, expect, it } from "vitest";
import { connectWebSocket, decodeFrame, encodeFrame, webSocketUrl } from "./websocket";

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

  it("is not wired up yet", () => {
    expect(() => connectWebSocket("/MaslowCNC")).toThrow("not implemented");
  });
});
