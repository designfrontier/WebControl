import { describe, expect, it } from "vitest";
import { z } from "zod";
import version from "../../../contract/VERSION?raw";
import { actionCommands, clientEvents, httpEndpoints, serverMessages } from "../protocol";
import { buildContract, toJsonSchema } from "./build";

const onDisk = import.meta.glob<string>("../../../contract/**/*.json", {
  query: "?raw",
  import: "default",
  eager: true,
});
const relative = (path: string) => path.replace("../../../contract/", "");
const generated = buildContract(version.trim());
const index = JSON.parse(generated["index.json"]);

describe("contract/", () => {
  it("is up to date with the zod schemas; run `npm run contract` after changing src/protocol", () => {
    const disk = Object.fromEntries(Object.entries(onDisk).map(([path, text]) => [relative(path), text]));
    expect(Object.keys(disk).sort()).toEqual(Object.keys(generated).sort());
    Object.entries(generated).forEach(([path, text]) => expect(disk[path], path).toBe(text));
  });

  it("lists every message, event, action and endpoint, each with a schema file", () => {
    const namespace = index.socket.namespaces["/MaslowCNC"];
    expect(Object.keys(namespace.serverToClient.message.commands)).toEqual(Object.keys(serverMessages));
    expect(Object.keys(namespace.clientToServer)).toEqual(Object.keys(clientEvents));
    expect(Object.keys(index.actions).sort()).toEqual([...actionCommands].sort());
    expect(index.http).toHaveLength(httpEndpoints.length);
    const referenced = JSON.stringify(index).match(/schemas\/[^"]+\.json/g) ?? [];
    expect(referenced.filter((path) => !(path in generated))).toEqual([]);
  });

  it("takes its version from contract/VERSION", () => {
    expect(index.version).toBe(Number(version));
    expect(Number.isInteger(index.version)).toBe(true);
  });
});

describe("toJsonSchema", () => {
  it("lets coerced numbers arrive as numeric text and keeps their description", () => {
    expect(toJsonSchema(z.coerce.number().describe("mm"), "x")).toMatchObject({
      title: "x",
      anyOf: [{ type: "number" }, { type: "string" }],
      description: "mm",
    });
  });

  it("lets coerced strings arrive as numbers", () => {
    expect(toJsonSchema(z.coerce.string(), "x")).toMatchObject({ type: ["string", "number"] });
  });
});
