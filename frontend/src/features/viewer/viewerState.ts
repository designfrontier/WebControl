import type { BoardInfo, Compressed, MessageData, ViewerSnapshot } from "../../protocol";
import { decodeCutCells, decodeGcode } from "./decode";
import { buildGcodeGeometry } from "./gcodeGeometry";
import type { GcodeGeometry } from "./gcodeGeometry";
import { DEFAULT_MACHINE, machineFromSnapshot } from "./shapes";
import type { MachineGeometry } from "./shapes";

export interface ViewerData {
  machine: MachineGeometry;
  gcode: GcodeGeometry | null;
  board: BoardInfo | null;
  cutCells: unknown[] | null;
  /** The server announced a payload (showFPSpinner) or the snapshot is loading. */
  loading: boolean;
  error: string | null;
  camera: { on: boolean; image: string | null };
}

export type ViewerEvent =
  | { type: "loading" }
  | { type: "snapshot"; snapshot: ViewerSnapshot }
  | { type: "snapshotFailed" }
  | { type: "gcode"; payload: Compressed }
  | { type: "cutData"; payload: Compressed }
  | { type: "board"; board: BoardInfo }
  | { type: "camera"; message: MessageData<"cameraMessage"> };

export const initialViewerData: ViewerData = {
  machine: DEFAULT_MACHINE,
  gcode: null,
  board: null,
  cutCells: null,
  loading: false,
  error: null,
  camera: { on: false, image: null },
};

const sameMachine = (a: MachineGeometry, b: MachineGeometry) =>
  (Object.keys(a) as (keyof MachineGeometry)[]).every((key) => a[key] === b[key]);

/** Decodes a payload, keeping the previous value and reporting `what` if it is corrupt. */
const decodeInto = <T>(state: ViewerData, what: string, decode: () => T, apply: (value: T) => Partial<ViewerData>) => {
  try {
    return { ...state, ...apply(decode()), loading: false, error: null };
  } catch (error) {
    console.warn(`Could not read the ${what}`, error);
    return { ...state, loading: false, error: `Could not read the ${what} from the server.` };
  }
};

const decodeGcodeGeometry = (payload: Compressed) => {
  const moves = decodeGcode(payload);
  return moves && buildGcodeGeometry(moves);
};

export const viewerReducer = (state: ViewerData, event: ViewerEvent): ViewerData => {
  switch (event.type) {
    case "loading":
      return state.loading ? state : { ...state, loading: true };
    case "snapshotFailed":
      return { ...state, loading: false, error: "Could not load the preview from the server." };
    case "snapshot": {
      const { snapshot } = event;
      const machine = machineFromSnapshot(snapshot);
      return decodeInto(
        state,
        "gcode preview",
        () => ({ gcode: decodeGcodeGeometry(snapshot.gcode), cutCells: decodeCutCells(snapshot.cutData) }),
        (decoded) => ({
          ...decoded,
          board: snapshot.board,
          machine: sameMachine(machine, state.machine) ? state.machine : machine,
        }),
      );
    }
    case "gcode":
      return decodeInto(
        state,
        "gcode preview",
        () => decodeGcodeGeometry(event.payload),
        (gcode) => ({ gcode }),
      );
    case "cutData":
      return decodeInto(
        state,
        "board cut data",
        () => decodeCutCells(event.payload),
        (cutCells) => ({ cutCells }),
      );
    case "board":
      return { ...state, board: event.board };
    case "camera":
      return event.message.command === "updateCamera"
        ? { ...state, camera: { on: event.message.data === "on", image: state.camera.image } }
        : { ...state, camera: { on: state.camera.on, image: event.message.data } };
  }
};
