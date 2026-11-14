import { describe, expect, it } from "vitest";
import actionsPy from "../../../Actions/actions.py?raw";
import mainPy from "../../../main.py?raw";
import baseSocketJs from "../../../static/scripts/baseSocket.js?raw";
import { actionCommands, clientEvents, decodeEnvelopeData, serverMessages } from ".";

const apiModules = import.meta.glob<string>("../../../api/*.py", { query: "?raw", import: "default", eager: true });

/** Commands the old client handles that no server code path emits any more. */
const NEVER_SENT = ["calibrationMessage", "gcodeUpdate", "closeContentModals"];

const pythonActionCommands = () =>
  actionsPy
    .replace(/'''[\s\S]*?'''/g, "")
    .split("\n")
    .filter((line) => !line.trim().startsWith("#"))
    .flatMap((line) => [...line.matchAll(/msg\["data"\]\["command"\] == "(\w+)"/g)].map((match) => match[1]));

describe("protocol coverage of the Python backend", () => {
  it("lists every command Actions.processAction handles, once", () => {
    expect([...actionCommands].sort()).toEqual([...new Set(pythonActionCommands())].sort());
    expect(new Set(actionCommands).size).toBe(actionCommands.length);
  });

  it("has a schema for every server message the old UI handled", () => {
    const handled = [...baseSocketJs.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/^\s*case '(\w+)':/gm)].map(
      (match) => match[1],
    );
    const missing = handled.filter((command) => !(command in serverMessages) && !NEVER_SENT.includes(command));
    expect(missing).toEqual([]);
  });

  it("only emits events the server has a /MaslowCNC handler for", () => {
    const handled = [mainPy, ...Object.values(apiModules)].flatMap((source) =>
      [...source.matchAll(/socketio\.on\(\s*"(\w+)",\s*namespace="\/MaslowCNC"/g)].map((match) => match[1]),
    );
    expect(Object.keys(apiModules)).toContain("../../../api/state.py");
    expect(Object.keys(clientEvents).filter((event) => !handled.includes(event))).toEqual([]);
  });
});

describe("decodeEnvelopeData", () => {
  it("parses JSON-encoded data", () => {
    expect(decodeEnvelopeData({ command: "x", data: '{"a":1}', dataFormat: "json" })).toEqual({ a: 1 });
  });

  it("passes other formats through", () => {
    expect(decodeEnvelopeData({ command: "showFPSpinner", data: 12, dataFormat: "int" })).toBe(12);
    expect(decodeEnvelopeData({ command: "x", data: "abc", dataFormat: "base64" })).toBe("abc");
  });

  it("returns undefined for broken JSON so validation rejects it", () => {
    expect(decodeEnvelopeData({ command: "x", data: "{nope", dataFormat: "json" })).toBeUndefined();
  });
});

describe("message schemas", () => {
  it("accepts home positions sent as config strings", () => {
    expect(serverMessages.homePositionMessage.parse({ xval: "12.5", yval: "-3" })).toEqual({ xval: 12.5, yval: -3 });
  });

  it("accepts the modal payload requestPage sends, client id included", () => {
    const modal = { title: "Help", message: "<p>hi</p>", isStatic: false, modalType: "content", client: "abc" };
    expect(serverMessages.activateModal.parse(modal)).toMatchObject(modal);
  });

  it("parses board info with numeric strings", () => {
    const board = serverMessages.boardDataUpdate.parse({
      width: "96",
      height: 48,
      thickness: 0.75,
      centerX: 0,
      centerY: 0,
      boardID: "b1",
      material: "Plywood",
      fileName: "",
    });
    expect(board.width).toBe(96);
  });
});
