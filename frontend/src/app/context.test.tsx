import { render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../test/render";
import { useClient, useFetch, useMachine, useMachineStore, useServerMessage } from "./context";

const Ports = () => {
  const [ports, setPorts] = useState<string[]>([]);
  useServerMessage("updatePorts", setPorts);
  return <p>Ports: {ports.join(", ")}</p>;
};

describe("context hooks", () => {
  it.each([
    ["useClient", useClient],
    ["useFetch", useFetch],
    ["useMachineStore", useMachineStore],
  ])("%s throws outside the provider", (name, hook) => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const Probe = () => {
      hook();
      return null;
    };
    expect(() => render(<Probe />)).toThrow(`${name} used outside <WebControlProvider>`);
  });

  it("useServerMessage delivers messages while mounted", () => {
    const { socket, unmount } = renderWithProviders(<Ports />);
    socket.serverMessage("updatePorts", ["/dev/ttyACM0", "/dev/ttyUSB0"]);
    expect(screen.getByText("Ports: /dev/ttyACM0, /dev/ttyUSB0")).toBeInTheDocument();
    unmount();
    expect(() => socket.serverMessage("updatePorts", ["COM1"])).not.toThrow();
  });

  it("useMachine re-renders on the selected slice", () => {
    const Units = () => <p>Units: {useMachine((s) => s.units) ?? "?"}</p>;
    const { socket } = renderWithProviders(<Units />, { fetch: () => new Promise(() => {}) });
    expect(screen.getByText("Units: ?")).toBeInTheDocument();
    socket.serverMessage("requestedSetting", { setting: "units", value: "INCHES" });
    expect(screen.getByText("Units: INCHES")).toBeInTheDocument();
  });
});
