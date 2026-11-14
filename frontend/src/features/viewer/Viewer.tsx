import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useClient, useFetch, useMachine, useServerMessage } from "../../app/context";
import { selectRunState } from "../../state/machine";
import { getViewerSnapshot } from "./api";
import { CameraOverlay } from "./CameraOverlay";
import { createViewerEngine } from "./engine";
import type { ViewerEngine } from "./engine";
import { segmentCount } from "./gcodeGeometry";
import { createTextLabel } from "./labels";
import { createViewerScene } from "./scene";
import type { ViewerScene } from "./scene";
import { ScreenActionDialog } from "./ScreenActionDialog";
import type { ScreenAction } from "./ScreenActionDialog";
import { isOnBed } from "./shapes";
import type { Point } from "./shapes";
import { fromInches, roundForAction, toInches, unitLabel } from "./units";
import { decodeCutPayload, decodeGcodePayload, decodeSnapshot, initialViewerData, viewerReducer } from "./viewerState";
import "./viewer.css";

interface View {
  scene: ViewerScene;
  engine: ViewerEngine;
}

/** The old touch.js waited 1.5 s before treating a touch as a context action. */
export const LONG_PRESS_MS = 1500;
const LONG_PRESS_SLOP_PX = 10;

const withView = (view: View | null, apply: (scene: ViewerScene, engine: ViewerEngine) => void) => {
  if (!view) return;
  apply(view.scene, view.engine);
  view.engine.requestRender();
};

const ToggleButton = ({ pressed, onToggle, children }: { pressed: boolean; onToggle(): void; children: string }) => (
  <button type="button" aria-pressed={pressed} onClick={onToggle}>
    {children}
  </button>
);

export const Viewer = () => {
  const client = useClient();
  const fetch = useFetch();
  const socket = useMachine((state) => state.socket);
  const units = useMachine((state) => state.units);
  const position = useMachine((state) => state.position);
  const homePosition = useMachine((state) => state.homePosition);
  const gcodePosition = useMachine((state) => state.gcodePosition);
  const positionError = useMachine((state) => state.positionError);
  const runState = useMachine(selectRunState);

  const [data, dispatch] = useReducer(viewerReducer, initialViewerData);
  const [view3D, setView3D] = useState(false);
  const [perspective, setPerspective] = useState(false);
  const [showBoard, setShowBoard] = useState(false);
  const [showLabels, setShowLabels] = useState(false);
  const [cursor, setCursor] = useState<Point | null>(null);
  const [target, setTarget] = useState<Point | null>(null);
  const [webglError, setWebglError] = useState<string | null>(null);

  const viewRef = useRef<View | null>(null);
  const pressRef = useRef<{ timer: number; x: number; y: number } | null>(null);
  const pointerTypeRef = useRef("mouse");

  const attachCanvas = useCallback((host: HTMLDivElement | null) => {
    if (!host) return;
    const scene = createViewerScene((text) => createTextLabel(text));
    try {
      viewRef.current = { scene, engine: createViewerEngine(host, scene.root) };
    } catch {
      scene.dispose();
      setWebglError("The 3D view needs WebGL, which this browser could not start.");
      return;
    }
    return () => {
      viewRef.current?.engine.dispose();
      viewRef.current?.scene.dispose();
      viewRef.current = null;
    };
  }, []);

  useServerMessage("showFPSpinner", () => dispatch({ type: "loading" }));
  useServerMessage("gcodeUpdateCompressed", (payload) =>
    dispatch({ type: "gcode", result: decodeGcodePayload(payload) }),
  );
  useServerMessage("boardCutDataUpdateCompressed", (payload) =>
    dispatch({ type: "cutData", result: decodeCutPayload(payload) }),
  );
  useServerMessage("boardDataUpdate", (board) => dispatch({ type: "board", board }));
  useServerMessage("cameraMessage", (message) => dispatch({ type: "camera", message }));

  useEffect(() => {
    if (socket !== "connected") return;
    let live = true;
    dispatch({ type: "snapshotStarted" });
    getViewerSnapshot(fetch).then(
      (snapshot) => live && dispatch({ type: "snapshot", snapshot: decodeSnapshot(snapshot) }),
      (error: unknown) => {
        console.warn("Could not load /api/viewer", error);
        if (live) dispatch({ type: "snapshotFailed" });
      },
    );
    client.sendAction("statusRequest", "cameraStatus");
    return () => {
      live = false;
    };
  }, [socket, client, fetch]);

  useEffect(
    () =>
      withView(viewRef.current, (scene, engine) => {
        scene.setMachine(data.machine);
        engine.fit(data.machine);
      }),
    [data.machine],
  );
  useEffect(() => withView(viewRef.current, (scene) => scene.setGcode(data.gcode)), [data.gcode]);
  useEffect(
    () => withView(viewRef.current, (scene) => scene.setBoard(data.board, data.cutCells)),
    [data.board, data.cutCells],
  );
  useEffect(() => withView(viewRef.current, (scene) => scene.setBoardVisible(showBoard)), [showBoard]);
  useEffect(() => withView(viewRef.current, (scene) => scene.setLabelsVisible(showLabels)), [showLabels]);
  useEffect(() => withView(viewRef.current, (_, engine) => engine.setRotate(view3D)), [view3D]);
  useEffect(() => withView(viewRef.current, (_, engine) => engine.setPerspective(perspective)), [perspective]);
  useEffect(() => withView(viewRef.current, (scene) => scene.setCursor(cursor)), [cursor]);
  useEffect(
    () =>
      withView(viewRef.current, (scene) =>
        scene.setSled(
          position && {
            x: toInches(position.x, units),
            y: toInches(position.y, units),
            z: toInches(position.z, units),
          },
        ),
      ),
    [position, units],
  );
  useEffect(
    () =>
      withView(viewRef.current, (scene) =>
        scene.setHome({ x: toInches(homePosition?.x ?? 0, units), y: toInches(homePosition?.y ?? 0, units) }),
      ),
    [homePosition, units],
  );
  useEffect(
    () =>
      withView(viewRef.current, (scene) =>
        scene.setGcodePosition(
          gcodePosition && {
            x: toInches(gcodePosition.x, units),
            y: toInches(gcodePosition.y, units),
            z: toInches(gcodePosition.z, units),
          },
        ),
      ),
    [gcodePosition, units],
  );
  useEffect(() => {
    const computed = positionError?.computed ?? null;
    // UIProcessor always reports the computed (forward kinematics) position in millimetres.
    withView(viewRef.current, (scene) =>
      scene.setComputedSled(computed && { x: toInches(computed.x, "MM"), y: toInches(computed.y, "MM") }),
    );
  }, [positionError]);

  const pick = (clientX: number, clientY: number) => viewRef.current?.engine.pick(clientX, clientY) ?? null;
  const openActions = (clientX: number, clientY: number) => {
    const point = pick(clientX, clientY);
    if (point) setTarget(point);
  };
  const cancelPress = () => {
    if (pressRef.current) window.clearTimeout(pressRef.current.timer);
    pressRef.current = null;
  };

  const onPointerDown = (event: ReactPointerEvent) => {
    pointerTypeRef.current = event.pointerType;
    cancelPress();
    if (event.pointerType !== "touch" || !event.isPrimary) return;
    const { clientX, clientY } = event;
    const timer = window.setTimeout(() => {
      pressRef.current = null;
      openActions(clientX, clientY);
    }, LONG_PRESS_MS);
    pressRef.current = { timer, x: clientX, y: clientY };
  };

  const onPointerMove = (event: ReactPointerEvent) => {
    const press = pressRef.current;
    if (press && Math.hypot(event.clientX - press.x, event.clientY - press.y) > LONG_PRESS_SLOP_PX) cancelPress();
    if (event.pointerType === "mouse") setCursor(pick(event.clientX, event.clientY));
  };

  useEffect(() => cancelPress, []);

  const runAction = (action: ScreenAction, point: Point) => {
    setTarget(null);
    if (action === "returnToCenter") return client.sendAction("returnToCenter");
    client.sendAction(action, roundForAction(point.x), roundForAction(point.y));
  };

  const cutSegments = data.gcode ? segmentCount(data.gcode.cuts) : 0;
  const rapidSegments = data.gcode ? segmentCount(data.gcode.rapids) : 0;
  const cursorText =
    cursor &&
    `Cursor X ${fromInches(cursor.x, units).toFixed(2)}, Y ${fromInches(cursor.y, units).toFixed(2)} ${unitLabel(units)}`;

  return (
    <figure
      className="viewer"
      aria-label="Gcode and board viewer"
      data-cut-segments={cutSegments}
      data-rapid-segments={rapidSegments}
    >
      <div className="viewer-toolbar" role="toolbar" aria-label="View">
        <ToggleButton pressed={view3D} onToggle={() => setView3D((on) => !on)}>
          3D
        </ToggleButton>
        <ToggleButton pressed={perspective} onToggle={() => setPerspective((on) => !on)}>
          Perspective
        </ToggleButton>
        <button type="button" onClick={() => withView(viewRef.current, (_, engine) => engine.reset())}>
          Reset view
        </button>
        <ToggleButton pressed={showBoard} onToggle={() => setShowBoard((on) => !on)}>
          Board
        </ToggleButton>
        <ToggleButton pressed={showLabels} onToggle={() => setShowLabels((on) => !on)}>
          Labels
        </ToggleButton>
        <ToggleButton pressed={data.camera.on} onToggle={() => client.sendAction("toggleCamera")}>
          Camera
        </ToggleButton>
      </div>

      <div className="viewer-stage">
        {webglError ? (
          <p className="viewer-message">{webglError}</p>
        ) : (
          <div
            ref={attachCanvas}
            className="viewer-canvas"
            role="img"
            aria-label="Bed, sled and toolpath. Right-click, or press and hold on a touch screen, for move and home actions."
            onContextMenu={(event) => {
              event.preventDefault();
              if (pointerTypeRef.current !== "touch" && !view3D) openActions(event.clientX, event.clientY);
            }}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={cancelPress}
            onPointerCancel={cancelPress}
            onPointerLeave={() => setCursor(null)}
          />
        )}
        <CameraOverlay key={String(data.camera.on)} on={data.camera.on} image={data.camera.image} />
        {data.loading ? (
          <div className="viewer-loading" role="status">
            <span className="viewer-spinner" aria-hidden="true" />
            Loading preview…
          </div>
        ) : null}
      </div>

      <figcaption className="viewer-caption">
        <span>{data.gcode ? `${cutSegments} cut and ${rapidSegments} rapid moves` : "No gcode loaded"}</span>
        {data.board ? (
          <span>
            Board: {data.board.boardID}, Material: {data.board.material}
          </span>
        ) : null}
        {cursorText ? <span>{cursorText}</span> : null}
        {data.error ? <span role="alert">{data.error}</span> : null}
      </figcaption>

      {target ? (
        <ScreenActionDialog
          point={target}
          units={units}
          onBed={isOnBed(data.machine, target)}
          runState={runState}
          onAction={runAction}
          onClose={() => setTarget(null)}
        />
      ) : null}
    </figure>
  );
};
