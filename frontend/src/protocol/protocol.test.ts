import { describe, expect, it } from "vitest";
import { decodeEnvelopeData, serverMessages } from ".";

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
