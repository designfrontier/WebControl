import { inflate } from "pako";
import type { z } from "zod";
import { cutCellsSchema, gcodeMovesSchema } from "../../protocol";
import type { Compressed, GcodeMove } from "../../protocol";

const base64ToBytes = (text: string) => Uint8Array.from(atob(text), (char) => char.charCodeAt(0));

const isEmpty = (payload: Compressed): payload is null =>
  payload === null || (typeof payload === "string" ? payload.length : payload.byteLength) === 0;

const toBytes = (payload: NonNullable<Compressed>) =>
  typeof payload === "string"
    ? base64ToBytes(payload)
    : payload instanceof Uint8Array
      ? payload
      : new Uint8Array(payload);

/** Inflates a gzip JSON payload and validates it; null when nothing is loaded. Throws on corrupt data. */
export const inflateJson = <S extends z.ZodType>(payload: Compressed, schema: S): z.output<S> | null =>
  isEmpty(payload) ? null : schema.parse(JSON.parse(new TextDecoder().decode(inflate(toBytes(payload)))));

export const decodeGcode = (payload: Compressed): GcodeMove[] | null => inflateJson(payload, gcodeMovesSchema);

export const decodeCutCells = (payload: Compressed): unknown[] | null => inflateJson(payload, cutCellsSchema);
