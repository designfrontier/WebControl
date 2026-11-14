import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { jsonResponse } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { pidSettings, systemFetch } from "../../test/systemFixtures";

const card = (name: string) => within(screen.getByRole("region", { name }));
const velocity = () => card("Velocity PID test");
const position = () => card("Position PID test");

const renderPid = (options: Parameters<typeof renderApp>[1] = {}) =>
  renderApp("/system/pid", { fetch: systemFetch(), ...options });
const loaded = () => screen.findByRole("region", { name: "Velocity PID test" });

const fill = async (
  user: ReturnType<typeof renderPid>["user"],
  form: () => ReturnType<typeof card>,
  values: object,
) => {
  await Object.entries(values).reduce(async (previous, [label, value]) => {
    await previous;
    const input = form().getByRole("textbox", { name: label });
    await user.clear(input);
    await user.type(input, String(value));
  }, Promise.resolve());
};

describe("PidTuning", () => {
  it("prefills the gains from the advanced settings", async () => {
    renderPid();
    await loaded();
    expect(velocity().getByRole("textbox", { name: "Kp" })).toHaveValue("5");
    expect(position().getByRole("textbox", { name: "Kd" })).toHaveValue("34");
    expect(position().getByRole("textbox", { name: "Time" })).toHaveValue("");
    expect(velocity().queryByRole("textbox", { name: "Time" })).not.toBeInTheDocument();
    expect(velocity().getByRole("radio", { name: "Top left" })).toBeChecked();
    expect(velocity().queryByRole("radio", { name: "Bottom left" })).not.toBeInTheDocument();
  });

  it("turns sprockets with the classic per-motor directions", async () => {
    const { user, socket } = renderPid();
    const left = within(await screen.findByRole("group", { name: "Top left motor clockwise" }));
    await user.click(left.getByRole("button", { name: "Clockwise 10" }));
    const right = within(screen.getByRole("group", { name: "Top right motor counter-clockwise" }));
    await user.click(right.getByRole("button", { name: "Counter-clockwise 0.1" }));
    const rightCw = within(screen.getByRole("group", { name: "Top right motor clockwise" }));
    await user.click(rightCw.getByRole("button", { name: "Clockwise 5" }));

    expect(socket.actions()).toEqual([
      ["rotateSprocket", "L", 10],
      ["rotateSprocket", "R", 0.1],
      ["rotateSprocket", "R", -5],
    ]);
  });

  it("shows all four motors for four-motor firmware", async () => {
    const { user, socket } = renderPid({ fetch: systemFetch({ "/api/system/pid": pidSettings({ fourMotor: true }) }) });
    const bottomRight = within(await screen.findByRole("group", { name: "Bottom right motor clockwise" }));
    await user.click(bottomRight.getByRole("button", { name: "Clockwise 1" }));
    const bottomLeft = within(screen.getByRole("group", { name: "Bottom left motor counter-clockwise" }));
    await user.click(bottomLeft.getByRole("button", { name: "Counter-clockwise 1" }));
    expect(socket.actions()).toEqual([
      ["rotateSprocket", "N", -1],
      ["rotateSprocket", "M", 1],
    ]);
    expect(velocity().getByRole("radio", { name: "Bottom right" })).toBeInTheDocument();
  });

  it("refuses to run with blank or non-numeric fields and marks them", async () => {
    const fetch = systemFetch();
    const { user } = renderPid({ fetch });
    await loaded();
    await fill(user, velocity, { Start: "1", Kp: "x" });
    await user.click(velocity().getByRole("button", { name: "Run velocity test" }));

    expect(velocity().getByRole("alert")).toHaveTextContent("Enter numbers for Stop, Steps, Kp.");
    expect(velocity().getByRole("textbox", { name: "Kp" })).toHaveAttribute("aria-invalid", "true");
    expect(velocity().getByRole("textbox", { name: "Start" })).not.toHaveAttribute("aria-invalid");
    expect(fetch).not.toHaveBeenCalledWith("/api/system/pid-test", expect.anything());
  });

  it("refuses forms the firmware cannot parse, as the server does", async () => {
    const { user } = renderPid();
    await loaded();
    await fill(user, velocity, { Start: "1e3", Stop: "0x10", Steps: "Infinity", Kp: ".5" });
    await user.click(velocity().getByRole("button", { name: "Run velocity test" }));
    expect(velocity().getByRole("alert")).toHaveTextContent("Enter numbers for Start, Stop, Steps.");
  });

  it("posts a position test with the chosen motor and version", async () => {
    const fetch = systemFetch({ "POST /api/system/pid-test": () => jsonResponse({ status: "sent" }, 202) });
    const { user } = renderPid({ fetch });
    await loaded();
    await user.click(position().getByRole("radio", { name: "Top right" }));
    await fill(user, position, { Start: "0", Stop: "100", Steps: "4", Time: "2" });
    await user.selectOptions(position().getByRole("combobox", { name: "Version" }), "2");
    await user.click(position().getByRole("button", { name: "Run position test" }));

    expect(await position().findByText(/Test sent/)).toBeInTheDocument();
    const [, init] = fetch.mock.calls.find(([url]) => url === "/api/system/pid-test")!;
    expect(JSON.parse(String(init?.body))).toEqual({
      kind: "position",
      motor: "R",
      start: "0",
      stop: "100",
      steps: "4",
      time: "2",
      version: "2",
      kp: "1300",
      ki: "0",
      kd: "34",
    });
  });

  it("reports a refused test", async () => {
    const fetch = systemFetch({ "POST /api/system/pid-test": () => jsonResponse({ error: "busy" }, 409) });
    const { user } = renderPid({ fetch });
    await loaded();
    await fill(user, velocity, { Start: "0", Stop: "1", Steps: "1" });
    await user.click(velocity().getByRole("button", { name: "Run velocity test" }));
    expect(await velocity().findByRole("alert")).toHaveTextContent(
      "Could not start the test: POST /api/system/pid-test failed with 409",
    );
  });

  it("charts results pushed by updatePIDData into the matching test", async () => {
    const { socket } = renderPid();
    await loaded();
    socket.serverMessage("updatePIDData", {
      command: "updatePIDData",
      data: { result: "velocity", version: "2", data: ["1,2,3", "2,3,4"] },
    });

    const chart = velocity().getByRole("img");
    expect(chart).toHaveAccessibleName("Velocity PID test results: Setpoint, Input, Output; 2 samples from 1 to 4");
    expect(
      within(velocity().getByRole("list"))
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["Setpoint", "Input", "Output"]);
    expect(position().getByText("No results yet.")).toBeInTheDocument();

    socket.serverMessage("updatePIDData", { command: "updatePIDData", data: { result: "torque" } });
    expect(position().getByText("No results yet.")).toBeInTheDocument();
  });

  it("labels a one-sample chart in the singular", async () => {
    const { socket } = renderPid();
    await loaded();
    socket.serverMessage("updatePIDData", { command: "x", data: { result: "position", version: 1, data: [1234.5] } });
    expect(position().getByRole("img")).toHaveAccessibleName(/1 sample from 1234 to 1236/);
  });

  it("disables motor moves and tests while a job is paused", async () => {
    renderPid({ machine: { status: { uploadFlag: 2, positioningMode: 0, currentTool: 0 } } });
    await loaded();
    expect(velocity().getByRole("button", { name: "Run velocity test" })).toBeDisabled();
    expect(screen.getAllByRole("button", { name: "Clockwise 10" })[0]).toBeDisabled();
  });

  it("offers a retry when the settings fail to load", async () => {
    let down = true;
    const respond = () => (down ? jsonResponse({}, 500) : jsonResponse(pidSettings()));
    const { user } = renderPid({ fetch: systemFetch({ "/api/system/pid": respond }) });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load PID settings");
    down = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await loaded();
  });
});
