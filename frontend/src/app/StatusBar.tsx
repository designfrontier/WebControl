import { useMachine } from "./context";
import type { MachineState } from "../state/machine";

const serverText = ({ socket, hostAddress }: Pick<MachineState, "socket" | "hostAddress">) =>
  socket === "connected"
    ? `Server: connected${hostAddress && hostAddress !== "-" ? ` (${hostAddress})` : ""}`
    : socket === "connecting"
      ? "Server: connecting…"
      : "Server: not connected";

const controllerText = ({ status, port, fakeServo }: MachineState["controller"]) =>
  status === "connected"
    ? `Controller: ${port}${fakeServo ? " / fake servo on" : ""}`
    : status === "disconnected"
      ? "Controller: not connected"
      : "Controller: unknown";

const toneFor = (status: MachineState["controller"]["status"]) =>
  status === "connected" ? "ok" : status === "disconnected" ? "bad" : undefined;

export const StatusBar = () => {
  const socket = useMachine((s) => s.socket);
  const hostAddress = useMachine((s) => s.hostAddress);
  const controller = useMachine((s) => s.controller);
  const health = useMachine((s) => s.health);

  return (
    <ul className="status-bar" aria-label="Status">
      <li className="chip" data-tone={socket === "connected" ? "ok" : socket === "connecting" ? undefined : "bad"}>
        {serverText({ socket, hostAddress })}
      </li>
      <li className="chip" data-tone={toneFor(controller.status)}>
        {controllerText(controller)}
      </li>
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
