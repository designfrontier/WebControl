import { z } from "zod";
import {
  LOGS_NAMESPACE,
  NAMESPACE,
  actionSpecs,
  clientEvents,
  compressedSchema,
  cutCellsSchema,
  envelopeSchema,
  gcodeMovesSchema,
  httpEndpoints,
  logMessageSchema,
  serverMessages,
} from "../protocol";
import type { BodyType, HttpBody, HttpEndpoint } from "../protocol";
import { frameSchema } from "../transport/websocket";

type Json = Record<string, unknown>;

/** Envelope `dataFormat` for the commands whose data is not JSON text. */
export const DATA_FORMATS: Record<string, "int" | "base64"> = {
  showFPSpinner: "int",
  gcodeUpdateCompressed: "base64",
  boardCutDataUpdateCompressed: "base64",
};

/** Socket events only the classic UI at / emits; a backend for the new UI need not handle them. */
export const CLASSIC_ONLY_EVENTS: Record<string, string[]> = {
  [NAMESPACE]: ["my event", "modalClosed", "contentModalClosed", "alertModalClosed", "requestPage"],
  "/WebMCP": ["checkInRequested"],
};

const NUMERIC_TEXT = "^\\s*[-+]?(\\d+\\.?\\d*|\\.\\d+)([eE][-+]?\\d+)?\\s*$";

const COMPRESSED = {
  anyOf: [
    {
      type: "string",
      contentEncoding: "base64",
      contentMediaType: "application/gzip",
      description: "gzip-compressed JSON; empty when nothing is loaded",
    },
    { type: "null" },
  ],
  description:
    "socket.io sends the gzip bytes as a binary attachment; HTTP and plain WebSocket send them as base64 text.",
};

const replace = (target: Json, next: Json) => {
  Object.keys(target).forEach((key) => delete target[key]);
  Object.assign(target, next);
};

/** Coerced numbers and strings accept what the backend really sends; compressed payloads get one shape. */
const override = ({ zodSchema, jsonSchema }: { zodSchema: unknown; jsonSchema: Json }) => {
  if (zodSchema === compressedSchema) return replace(jsonSchema, COMPRESSED);
  const { def } = zodSchema as z.ZodType;
  if (!("coerce" in def) || !def.coerce) return;
  const { description } = jsonSchema;
  const alternatives =
    def.type === "number"
      ? [{ type: "number" }, { type: "string", pattern: NUMERIC_TEXT }]
      : [{ type: "string" }, { type: "number" }];
  replace(jsonSchema, { anyOf: alternatives, ...(description ? { description } : {}) });
};

export const toJsonSchema = (schema: z.ZodType, title: string): Json => {
  const { $schema, ...rest } = z.toJSONSchema(schema, { io: "input", unrepresentable: "any", override }) as Json;
  return { $schema, title, ...rest };
};

const slug = (endpoint: HttpEndpoint) =>
  `${endpoint.method}${endpoint.path}`
    .replace(/[{}]/g, "")
    .replace(/[^A-Za-z0-9]+/g, "-")
    .replace(/-$/, "")
    .toLowerCase();

const CONTENT_TYPES: Record<BodyType, string> = {
  json: "application/json",
  form: "multipart/form-data",
  text: "text/plain",
  html: "text/html",
  file: "application/octet-stream",
};

/** Builds every generated file under contract/, keyed by path relative to it. */
export const buildContract = (version: string): Record<string, string> => {
  const files: Record<string, Json> = {};
  const add = (path: string, schema: z.ZodType, title: string) => {
    files[path] = toJsonSchema(schema, title);
    return path;
  };

  const messages = Object.fromEntries(
    Object.entries(serverMessages).map(([command, schema]) => [
      command,
      {
        schema: add(`schemas/messages/${command}.json`, schema, command),
        dataFormat: DATA_FORMATS[command] ?? "json",
      },
    ]),
  );
  const events = Object.fromEntries(
    Object.entries(clientEvents).map(([event, schema]) => [
      event,
      { schema: add(`schemas/events/${event}.json`, schema, event) },
    ]),
  );
  const actions = Object.fromEntries(
    Object.entries(actionSpecs).map(([command, { allowedDuring, via, summary, arg, arg1 }]) => [
      command,
      {
        summary,
        allowedDuring,
        via,
        schema: add(
          `schemas/actions/${command}.json`,
          z.object({ data: z.object({ command: z.literal(command), arg, arg1 }) }),
          command,
        ),
      },
    ]),
  );

  const bodyEntry = (base: string, body: HttpBody) => ({
    contentType: CONTENT_TYPES[body.type],
    ...(body.description ? { description: body.description } : {}),
    ...(body.schema ? { schema: add(`${base}.json`, body.schema, base.split("/").pop()!) } : {}),
  });
  const http = httpEndpoints.map((endpoint) => {
    const base = `schemas/http/${slug(endpoint)}`;
    const { method, path, family, summary, params, query, body, responses } = endpoint;
    return {
      method,
      path,
      family,
      summary,
      ...(params ? { params: add(`${base}.params.json`, params, `${slug(endpoint)}.params`) } : {}),
      ...(query ? { query: add(`${base}.query.json`, query, `${slug(endpoint)}.query`) } : {}),
      ...(body ? { body: bodyEntry(`${base}.request`, body) } : {}),
      responses: Object.fromEntries(
        Object.entries(responses).map(([status, response]) => [status, bodyEntry(`${base}.${status}`, response)]),
      ),
    };
  });

  const index = {
    version: Number(version),
    generatedFrom: "frontend/src/protocol (npm run contract)",
    socket: {
      path: "/socket.io",
      namespaces: {
        [NAMESPACE]: {
          serverToClient: {
            message: { envelope: add("schemas/socket/envelope.json", envelopeSchema, "envelope"), commands: messages },
          },
          clientToServer: events,
          classicOnlyClientToServer: CLASSIC_ONLY_EVENTS[NAMESPACE],
        },
        [LOGS_NAMESPACE]: {
          serverToClient: { message: { schema: add("schemas/logs/message.json", logMessageSchema, "logMessage") } },
          clientToServer: {},
        },
        "/WebMCP": {
          serverToClient: {},
          clientToServer: {},
          classicOnlyClientToServer: CLASSIC_ONLY_EVENTS["/WebMCP"],
        },
      },
    },
    actions,
    http,
    payloads: {
      gcodeUpdateCompressed: add("schemas/payloads/gcodeMoves.json", gcodeMovesSchema, "gcodeMoves"),
      boardCutDataUpdateCompressed: add("schemas/payloads/cutCells.json", cutCellsSchema, "cutCells"),
    },
    websocket: { frame: add("schemas/websocket/frame.json", frameSchema, "frame") },
  };

  const text = (value: unknown) => `${JSON.stringify(value, null, 2)}\n`;
  return {
    "index.json": text(index),
    ...Object.fromEntries(Object.entries(files).map(([path, schema]) => [path, text(schema)])),
  };
};
