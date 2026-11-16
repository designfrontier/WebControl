import { useMachine } from "./context";
import type { MachineState } from "../state/machine";

type Tone = "ok" | "bad" | undefined;

const serverText = (socket: MachineState["socket"]) =>
  socket === "connected"
    ? "Server: connected"
    : socket === "connecting"
      ? "Server: connecting…"
      : "Server: not connected";

/** The server's own address, which it reports as "-" until it knows it. */
const hostTitle = (hostAddress: string | null) =>
  hostAddress && hostAddress !== "-" ? `WebControl at ${hostAddress}` : undefined;

const fakeServoText = (fakeServo: boolean) => (fakeServo ? " / fake servo on" : "");

const controllerText = ({ status, port, fakeServo }: MachineState["controller"]) =>
  status === "connected"
    ? `Controller: ${port}${fakeServoText(fakeServo)}`
    : status === "disconnected"
      ? "Controller: not connected"
      : "Controller: unknown";

/** The backend's firmware ranges: below 50 is stock, 50 up to 100 holey, 100 and above custom. */
const firmwareText = (version: number | null) =>
  version === null ? "Firmware: unknown" : `${version < 50 ? "Stock" : version < 100 ? "Holey" : "Custom"} ${version}`;

const softwareText = ({ version, commit }: NonNullable<MachineState["software"]>) =>
  `WebControl ${version}${commit ? ` · ${commit}` : ""}`;

const toneFor = (status: MachineState["controller"]["status"]): Tone =>
  status === "connected" ? "ok" : status === "disconnected" ? "bad" : undefined;

interface ChipProps {
  text: string;
  /** What a narrow header shows instead, and `shortest` a narrower one; screen readers always get `text`. */
  short?: string;
  shortest?: string;
  tone?: Tone;
  title?: string;
  kind?: "state" | "software";
}

const Chip = ({ text, short, shortest, tone, title, kind }: ChipProps) => (
  <li
    className="chip"
    data-tone={tone}
    data-kind={kind}
    data-short={short !== undefined || undefined}
    data-shortest={shortest !== undefined || undefined}
    title={title}
  >
    {short === undefined && shortest === undefined ? (
      text
    ) : (
      <>
        <span className="chip-full">{text}</span>
        {short === undefined ? null : (
          <span className="chip-short" aria-hidden="true">
            {short}
          </span>
        )}
        {shortest === undefined ? null : (
          <span className="chip-shortest" aria-hidden="true">
            {shortest}
          </span>
        )}
      </>
    )}
  </li>
);

export const StatusBar = () => {
  const socket = useMachine((s) => s.socket);
  const hostAddress = useMachine((s) => s.hostAddress);
  const controller = useMachine((s) => s.controller);
  const health = useMachine((s) => s.health);
  const firmware = useMachine((s) => s.firmwareVersion);
  const software = useMachine((s) => s.software);

  return (
    <ul className="status-bar" aria-label="Status">
      <Chip
        text={serverText(socket)}
        short="Server"
        tone={socket === "connected" ? "ok" : socket === "connecting" ? undefined : "bad"}
        title={hostTitle(hostAddress)}
        kind="state"
      />
      <Chip
        text={controllerText(controller)}
        short={
          controller.status === "connected" ? `${controller.port}${fakeServoText(controller.fakeServo)}` : "Controller"
        }
        tone={toneFor(controller.status)}
        kind="state"
      />
      <Chip
        text={firmwareText(firmware)}
        shortest={firmware === null ? undefined : String(firmware)}
        title="Controller firmware"
      />
      {software && (
        <Chip
          text={softwareText(software)}
          short={`v${software.version}${software.commit ? `·${software.commit}` : ""}`}
          shortest={software.commit ?? `v${software.version}`}
          title={`${softwareText(software)}: the WebControl version and build`}
          kind="software"
        />
      )}
      {health && (
        <Chip
          text={`CPU ${Math.round(health.cpuUsage)}%`}
          shortest={`${Math.round(health.cpuUsage)}%`}
          title="Server CPU use"
        />
      )}
      {health && health.bufferSize >= 0 && <Chip text={`Buffer ${health.bufferSize}`} />}
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
