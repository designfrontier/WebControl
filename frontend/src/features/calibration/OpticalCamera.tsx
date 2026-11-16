import { useEffect, useState } from "react";
import { useClient, useFetch, useServerMessage } from "../../app/context";
import type { OpticalSettings, OpticalState } from "../../protocol";
import { captureTestImage, findOpticalCenter } from "./opticalApi";
import type { Status } from "./StatusLine";

interface Props {
  camera: OpticalState["camera"];
  /** The form's settings, or null (after flagging the bad fields) when they do not parse. */
  settings: () => OpticalSettings | null;
  onCentre: (x: number, y: number) => void;
  onStatus: (status: Status) => void;
}

const png = (data: string) => `data:image/png;base64,${data}`;

/** The live view, a test image with what the detector found, and find centre. */
export const OpticalCamera = ({ camera, settings, onCentre, onStatus }: Props) => {
  const client = useClient();
  const fetchImpl = useFetch();
  const [live, setLive] = useState<{ on: boolean; image: string | null }>({ on: false, image: null });
  const [test, setTest] = useState<string | null>(null);

  useEffect(() => client.sendAction("queryCamera"), [client]);
  useServerMessage("cameraMessage", (message) =>
    setLive((previous) =>
      message.command === "updateCamera"
        ? { on: message.data === "on", image: message.data === "on" ? previous.image : null }
        : { ...previous, image: message.data },
    ),
  );

  const testImage = async () => {
    const valid = settings();
    if (!valid) return;
    onStatus({ tone: "info", message: "Capturing a test image…" });
    const outcome = await captureTestImage(fetchImpl, valid);
    if (!outcome.ok) return onStatus({ tone: "error", message: outcome.error });
    setTest(outcome.data.png);
    const found = outcome.data.measurement;
    onStatus(
      found
        ? {
            tone: "ok",
            message: `Found the square ${found.dxMm.toFixed(2)} mm right and ${found.dyMm.toFixed(2)} mm up of the centre, turned ${found.angleDeg.toFixed(1)}°.`,
          }
        : {
            tone: "error",
            message: "No square found in the test image. Check the focus, light and recognition settings.",
          },
    );
  };

  const findCentre = async () => {
    const valid = settings();
    if (!valid) return;
    onStatus({ tone: "info", message: "Looking for the square under the camera…" });
    const outcome = await findOpticalCenter(fetchImpl, valid);
    if (!outcome.ok) return onStatus({ tone: "error", message: outcome.error });
    onCentre(outcome.data.opticalCenterX, outcome.data.opticalCenterY);
    onStatus({ tone: "ok", message: "Found the centre. Save the settings to keep it." });
  };

  return (
    <div className="card">
      <h3>Camera</h3>
      <p>
        Source: <code>{camera.source}</code>
        {camera.available ? "" : " (not found; set Camera Source in Settings > Camera Settings)"}
      </p>
      <div className="button-row">
        <button type="button" onClick={() => client.sendAction("toggleCamera")}>
          {live.on ? "Stop live view" : "Start live view"}
        </button>
        <button type="button" onClick={testImage}>
          Test image
        </button>
        <button type="button" onClick={findCentre}>
          Find centre
        </button>
      </div>
      <div className="camera-images">
        {live.on && live.image ? (
          <img className="camera-image" src={png(live.image)} alt="Live camera view" />
        ) : (
          <p className="muted">Live view is off.</p>
        )}
        {test && <img className="camera-image" src={png(test)} alt="Test image" />}
      </div>
    </div>
  );
};
