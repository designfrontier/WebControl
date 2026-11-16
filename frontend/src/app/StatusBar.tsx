import { useMachine } from "./context";
import type { MachineState } from "../state/machine";

const serverText = (socket: MachineState["socket"]) =>
  socket === "connected"
    ? "Server: connected"
    : socket === "connecting"
      ? "Server: connecting…"
      : "Server: not connected";

/** The server's own address, which it reports as "-" until it knows it. */
const hostTitle = (hostAddress: string | null) =>
  hostAddress && hostAddress !== "-" ? `WebControl at ${hostAddress}` : undefined;

const controllerText = ({ status, port, fakeServo }: MachineState["controller"]) =>
  status === "connected"
    ? `Controller: ${port}${fakeServo ? " / fake servo on" : ""}`
    : status === "disconnected"
      ? "Controller: not connected"
      : "Controller: unknown";

/** The backend's firmware ranges: below 50 is stock, 50 up to 100 holey, 100 and above custom. */
const firmwareText = (version: number | null) =>
  version === null ? "Firmware: unknown" : `${version < 50 ? "Stock" : version < 100 ? "Holey" : "Custom"} ${version}`;

const softwareText = ({ version, commit }: NonNullable<MachineState["software"]>) =>
  `WebControl ${version}${commit ? ` · ${commit}` : ""}`;

const toneFor = (status: MachineState["controller"]["status"]) =>
  status === "connected" ? "ok" : status === "disconnected" ? "bad" : undefined;

export const StatusBar = () => {
  const socket = useMachine((s) => s.socket);
  const hostAddress = useMachine((s) => s.hostAddress);
  const controller = useMachine((s) => s.controller);
  const health = useMachine((s) => s.health);
  const firmware = useMachine((s) => s.firmwareVersion);
  const software = useMachine((s) => s.software);

  return (
    <ul className="status-bar" aria-label="Status">
      <li
        className="chip"
        title={hostTitle(hostAddress)}
        data-tone={socket === "connected" ? "ok" : socket === "connecting" ? undefined : "bad"}
      >
        {serverText(socket)}
      </li>
      <li className="chip" data-tone={toneFor(controller.status)}>
        {controllerText(controller)}
      </li>
      <li className="chip" title="Controller firmware">
        {firmwareText(firmware)}
      </li>
      {software && (
        <li className="chip" title="WebControl version and build">
          {softwareText(software)}
        </li>
      )}
      {health && <li className="chip">CPU {Math.round(health.cpuUsage)}%</li>}
      {health && health.bufferSize >= 0 && <li className="chip">Buffer {health.bufferSize}</li>}
    </ul>
  );
};

export const AlarmBanner = () => {
  const alarm = useMachine((s) => s.alarm);
  if (!alarm) return null;
  return (
    <p className="alarm" role="alert">
      {alarm}
    </p>
  );
};
