import type { BoardInfo, Compressed, MessageData, ViewerSnapshot } from "../../protocol";
import { decodeCutCells, decodeGcode } from "./decode";
import { buildGcodeGeometry } from "./gcodeGeometry";
import type { GcodeGeometry } from "./gcodeGeometry";
import { DEFAULT_MACHINE, machineFromSnapshot } from "./shapes";
import type { MachineGeometry } from "./shapes";

type Pushable = "gcode" | "cutCells" | "board";

export interface ViewerData {
  machine: MachineGeometry;
  gcode: GcodeGeometry | null;
  board: BoardInfo | null;
  cutCells: unknown[] | null;
  /** The server announced a payload (showFPSpinner) or the snapshot is loading. */
  loading: boolean;
  error: string | null;
  camera: { on: boolean; image: string | null };
  /** Fields a socket push set since the snapshot fetch began, which the older snapshot must not overwrite. */
  pushed: Record<Pushable, boolean>;
}

/** A decoded payload, or the message to show when it was corrupt. */
export type Decoded<T> = { value: T } | { error: string };

export interface DecodedSnapshot {
  machine: MachineGeometry;
  board: BoardInfo | null;
  gcode: Decoded<GcodeGeometry | null>;
  cutCells: Decoded<unknown[] | null>;
}

export type ViewerEvent =
  | { type: "loading" }
  | { type: "snapshotStarted" }
  | { type: "snapshot"; snapshot: DecodedSnapshot }
  | { type: "snapshotFailed" }
  | { type: "gcode"; result: Decoded<GcodeGeometry | null> }
  | { type: "cutData"; result: Decoded<unknown[] | null> }
  | { type: "board"; board: BoardInfo }
  | { type: "camera"; message: MessageData<"cameraMessage"> };

const nothingPushed: Record<Pushable, boolean> = { gcode: false, cutCells: false, board: false };

export const initialViewerData: ViewerData = {
  machine: DEFAULT_MACHINE,
  gcode: null,
  board: null,
  cutCells: null,
  loading: false,
  error: null,
  camera: { on: false, image: null },
  pushed: nothingPushed,
};

const tryDecode = <T>(what: string, decode: () => T): Decoded<T> => {
  try {
    return { value: decode() };
  } catch (error) {
    console.warn(`Could not read the ${what}`, error);
    return { error: `Could not read the ${what} from the server.` };
  }
};

export const decodeGcodePayload = (payload: Compressed) =>
  tryDecode("gcode preview", () => {
    const moves = decodeGcode(payload);
    return moves && buildGcodeGeometry(moves);
  });

export const decodeCutPayload = (payload: Compressed) => tryDecode("board cut data", () => decodeCutCells(payload));

export const decodeSnapshot = (snapshot: ViewerSnapshot): DecodedSnapshot => ({
  machine: machineFromSnapshot(snapshot),
  board: snapshot.board,
  gcode: decodeGcodePayload(snapshot.gcode),
  cutCells: decodeCutPayload(snapshot.cutData),
});

const sameMachine = (a: MachineGeometry, b: MachineGeometry) =>
  (Object.keys(a) as (keyof MachineGeometry)[]).every((key) => a[key] === b[key]);

/** Applies a decoded value, or keeps the previous one and reports the error. */
const applyDecoded = <K extends "gcode" | "cutCells">(
  state: ViewerData,
  key: K,
  result: Decoded<ViewerData[K]>,
): ViewerData => ("error" in result ? { ...state, error: result.error } : { ...state, [key]: result.value });

const applyPush = <K extends "gcode" | "cutCells">(state: ViewerData, key: K, result: Decoded<ViewerData[K]>) =>
  applyDecoded({ ...state, loading: false, error: null, pushed: { ...state.pushed, [key]: true } }, key, result);

export const viewerReducer = (state: ViewerData, event: ViewerEvent): ViewerData => {
  switch (event.type) {
    case "loading":
      return state.loading ? state : { ...state, loading: true };
    case "snapshotStarted":
      return { ...state, loading: true, pushed: nothingPushed };
    case "snapshotFailed":
      return { ...state, loading: false, error: "Could not load the preview from the server." };
    case "snapshot": {
      const { snapshot } = event;
      const { pushed } = state;
      const base: ViewerData = {
        ...state,
        machine: sameMachine(snapshot.machine, state.machine) ? state.machine : snapshot.machine,
        board: pushed.board ? state.board : snapshot.board,
        loading: false,
        error: null,
      };
      const withGcode = pushed.gcode ? base : applyDecoded(base, "gcode", snapshot.gcode);
      return pushed.cutCells ? withGcode : applyDecoded(withGcode, "cutCells", snapshot.cutCells);
    }
    case "gcode":
      return applyPush(state, "gcode", event.result);
    case "cutData":
      return applyPush(state, "cutCells", event.result);
    case "board":
      return { ...state, board: event.board, pushed: { ...state.pushed, board: true } };
    case "camera":
      return event.message.command === "updateCamera"
        ? {
            ...state,
            camera: event.message.data === "on" ? { on: true, image: state.camera.image } : { on: false, image: null },
          }
        : { ...state, camera: { on: state.camera.on, image: event.message.data } };
  }
};
