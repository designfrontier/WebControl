import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { SettingsSection } from "../../protocol";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { gpioSection, maslowSection, setting, webcontrolSection } from "./fixtures";

const sentBody = (init: RequestInit | undefined) =>
  JSON.parse(String(init?.body)) as { values: Record<string, unknown> };

/** A server that applies PUTs to its copy of the section, recording each body. */
const settingsServer = (section: SettingsSection = maslowSection()) => {
  const puts: Record<string, unknown>[] = [];
  let current = section;
  const fetch = fakeFetch({
    [`GET /api/settings/${section.id}`]: () => jsonResponse(current),
    [`PUT /api/settings/${section.id}`]: (init) => {
      const { values } = sentBody(init);
      puts.push(values);
      current = {
        ...current,
        settings: current.settings.map((s) => (s.key in values ? { ...s, value: values[s.key] as string } : s)),
      };
      return jsonResponse(current);
    },
  });
  return { fetch, puts };
};

const form = () => screen.findByRole("form", { name: "Maslow Settings" });
const saveButton = () => screen.getByRole("button", { name: "Save" });
const notice = () => within(screen.getByRole("form")).getByRole("status");

describe("SettingsPage", () => {
  it("opens on the Maslow section with a tab for each old settings modal", async () => {
    const { fetch } = settingsServer();
    renderApp("/settings", { fetch });
    expect(screen.getByText("Loading settings…")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Maslow", level: 3 })).toBeInTheDocument();

    const tabs = within(screen.getByRole("navigation", { name: "Settings sections" }));
    expect(tabs.getAllByRole("link").map((link) => link.textContent)).toEqual([
      "Maslow",
      "Advanced",
      "WebControl",
      "Camera",
      "GPIO",
      "Display",
    ]);
    expect(tabs.getByRole("link", { name: "Maslow" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByLabelText("Distance Between Motors")).toHaveValue("2978.4");
    expect(screen.getByLabelText("Distance Between Motors")).toHaveAccessibleDescription(
      "The horizontal distance.\ndefault setting: 2978.4",
    );
    expect(screen.getByLabelText("Z-axis installed")).not.toBeChecked();
    expect(screen.getByLabelText("Color Scheme")).toHaveValue("Light");
    expect(screen.getByLabelText("Serial Connection")).toHaveValue("/dev/ttyACM0");
  });

  it("saves only the changed settings and shows the saved values", async () => {
    const { fetch, puts } = settingsServer();
    const { user } = renderApp("/settings", { fetch });
    await form();
    expect(saveButton()).toBeDisabled();

    const spacing = screen.getByLabelText("Distance Between Motors");
    await user.clear(spacing);
    await user.type(spacing, "3000");
    await user.click(screen.getByLabelText("Z-axis installed"));
    await user.selectOptions(screen.getByLabelText("Color Scheme"), "Dark");
    expect(notice()).toHaveTextContent("Unsaved changes.");
    await user.click(saveButton());

    expect(await screen.findByText("Maslow Settings saved.")).toBeInTheDocument();
    expect(puts).toEqual([{ motorSpacingX: "3000", zAxis: true, colorScheme: "Dark" }]);
    expect(screen.getByLabelText("Distance Between Motors")).toHaveValue("3000");
    expect(saveButton()).toBeDisabled();
  });

  it("sends a decimal comma as a point and leaves text settings as typed", async () => {
    const { fetch, puts } = settingsServer();
    const { user } = renderApp("/settings", { fetch });
    await form();
    const spacing = screen.getByLabelText("Distance Between Motors");
    await user.clear(spacing);
    await user.type(spacing, "2978,5");
    await user.clear(screen.getByLabelText("Macro 1 Title"));
    await user.type(screen.getByLabelText("Macro 1 Title"), "1,5");
    await user.click(saveButton());

    expect(await screen.findByText("Maslow Settings saved.")).toBeInTheDocument();
    expect(puts).toEqual([{ motorSpacingX: "2978.5", macro1_title: "1,5" }]);
  });

  it("flags invalid numbers without saving, and clears the flag on edit", async () => {
    const { fetch, puts } = settingsServer();
    const { user } = renderApp("/settings", { fetch });
    await form();

    const spacing = screen.getByLabelText("Distance Between Motors");
    await user.clear(spacing);
    await user.type(spacing, "wide");
    await user.click(saveButton());

    expect(spacing).toHaveAttribute("aria-invalid", "true");
    expect(spacing).toHaveAccessibleDescription(expect.stringContaining("Enter a number"));
    expect(notice()).toHaveTextContent("Fix the highlighted settings.");
    expect(puts).toEqual([]);

    await user.type(spacing, "1");
    expect(spacing).not.toHaveAttribute("aria-invalid");
  });

  it("discards unsaved edits", async () => {
    const { fetch } = settingsServer();
    const { user } = renderApp("/settings", { fetch });
    await form();
    const title = screen.getByLabelText("Macro 1 Title");
    await user.type(title, "!");
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(title).toHaveValue("Macro 1");
    expect(screen.getByRole("button", { name: "Discard changes" })).toBeDisabled();
  });

  it("refreshes the serial port list and keeps the chosen port", async () => {
    const { fetch } = settingsServer();
    const { user, socket } = renderApp("/settings", { fetch });
    await form();
    const ports = screen.getByLabelText("Serial Connection");
    expect(
      within(ports)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["/dev/ttyACM0", "/dev/ttyACM1"]);

    await user.click(screen.getByRole("button", { name: "Refresh ports" }));
    expect(socket.actions()).toContainEqual(["updatePorts", "", ""]);

    socket.serverMessage("updatePorts", ["/dev/ttyUSB0"]);
    expect(
      within(ports)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["/dev/ttyACM0", "/dev/ttyUSB0"]);
    expect(ports).toHaveValue("/dev/ttyACM0");
  });

  it("keeps Save disabled during a job, saying why", async () => {
    const { fetch } = settingsServer();
    const locked = fakeFetch({ "GET /api/state": startupState({ jobInProgress: true }) });
    const { user } = renderApp("/settings", {
      fetch: (input, init) => (String(input) === "/api/state" ? locked(input, init) : fetch(input, init)),
    });
    await form();
    expect(
      await screen.findByText(/A job is running or paused. Stop the job first to change the settings/),
    ).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("Serial Connection"), "/dev/ttyACM1");
    expect(saveButton()).toBeDisabled();
  });

  it("shows the server's reason when a save is refused during a job", async () => {
    const fetch = fakeFetch({
      "GET /api/settings/maslow": maslowSection(),
      "PUT /api/settings/maslow": () =>
        jsonResponse({ error: "Cannot change settings while a job is running or paused. Stop the job first." }, 409),
    });
    const { user } = renderApp("/settings", { fetch });
    await form();
    await user.type(screen.getByLabelText("Distance Between Motors"), "1");
    await user.click(saveButton());
    expect(
      await screen.findByText(
        "Saving failed: Cannot change settings while a job is running or paused. Stop the job first.",
      ),
    ).toBeInTheDocument();
  });

  it("shows an unset port before any scan", async () => {
    const section = maslowSection({
      ports: undefined,
      settings: [setting({ key: "COMport", title: "Serial Connection" })],
    });
    renderApp("/settings", { fetch: settingsServer(section).fetch });
    await form();
    expect(screen.getByRole("option", { name: "Not set" })).toBeInTheDocument();
  });

  it("locks custom-firmware settings until the controller has custom firmware", async () => {
    const locked = maslowSection({
      settings: [setting({ key: "a", custom: true }), setting({ key: "b", title: "Bee", custom: true })],
    });
    renderApp("/settings", { fetch: settingsServer(locked).fetch });
    await form();
    expect(screen.getByText(/2 settings are locked/)).toBeInTheDocument();
    expect(screen.getByLabelText("Bee")).toBeDisabled();
    expect(screen.getByLabelText("Bee")).toHaveAccessibleDescription(expect.stringContaining("Needs custom firmware"));
  });

  it("counts a single locked setting", async () => {
    const locked = maslowSection({ settings: [setting({ key: "a", custom: true }), setting({ key: "b" })] });
    renderApp("/settings", { fetch: settingsServer(locked).fetch });
    await form();
    expect(screen.getByText(/1 setting is locked/)).toBeInTheDocument();
    expect(screen.getByLabelText("b")).toBeEnabled();
  });

  it("unlocks custom settings with custom firmware", async () => {
    const unlocked = maslowSection({ enableCustom: true, settings: [setting({ key: "a", custom: true })] });
    renderApp("/settings", { fetch: settingsServer(unlocked).fetch });
    await form();
    expect(screen.queryByText(/locked until/)).not.toBeInTheDocument();
    expect(screen.getByLabelText("a")).toBeEnabled();
  });

  it("reports a rejected save and keeps the edits", async () => {
    const fetch = fakeFetch({
      "GET /api/settings/maslow": maslowSection(),
      "PUT /api/settings/maslow": () => jsonResponse({ errors: { macro1_title: "nope" } }, 400),
    });
    const { user } = renderApp("/settings", { fetch });
    await form();
    await user.type(screen.getByLabelText("Macro 1 Title"), "!");
    await user.click(saveButton());
    expect(await screen.findByText("The server rejected these values; nothing was saved.")).toBeInTheDocument();
    expect(screen.getByLabelText("Macro 1 Title")).toHaveValue("Macro 1!");
    expect(screen.getByLabelText("Macro 1 Title")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("nope")).toBeInTheDocument();
  });

  it("shows no field errors when a rejection has no per-setting reasons", async () => {
    const fetch = fakeFetch({
      "GET /api/settings/maslow": maslowSection(),
      "PUT /api/settings/maslow": () => jsonResponse({ error: "Expected values" }, 400),
    });
    const { user } = renderApp("/settings", { fetch });
    await form();
    await user.type(screen.getByLabelText("Macro 1 Title"), "!");
    await user.click(saveButton());
    expect(await screen.findByText("The server rejected these values; nothing was saved.")).toBeInTheDocument();
    expect(screen.getByLabelText("Macro 1 Title")).not.toHaveAttribute("aria-invalid");
  });

  it("reports a failed save", async () => {
    const fetch = fakeFetch({
      "GET /api/settings/maslow": maslowSection(),
      "PUT /api/settings/maslow": () => jsonResponse({}, 500),
    });
    const { user } = renderApp("/settings", { fetch });
    await form();
    await user.type(screen.getByLabelText("Macro 1 Title"), "!");
    await user.click(saveButton());
    expect(await screen.findByText("Saving failed: PUT /api/settings/maslow failed with 500")).toBeInTheDocument();
    expect(saveButton()).toBeEnabled();
  });

  it("disables the form buttons while saving", async () => {
    let finish: (response: Response) => void = () => {};
    const fetch = fakeFetch({
      "GET /api/settings/maslow": maslowSection(),
      "PUT /api/settings/maslow": () => new Promise<Response>((resolve) => (finish = resolve)),
    });
    const { user } = renderApp("/settings", { fetch });
    await form();
    await user.type(screen.getByLabelText("Macro 1 Title"), "!");
    await user.click(saveButton());
    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Discard changes" })).toBeDisabled();
    finish(jsonResponse(maslowSection()));
    expect(await screen.findByText("Maslow Settings saved.")).toBeInTheDocument();
  });

  it("offers a retry when a section fails to load", async () => {
    let healthy = false;
    const load = () => (healthy ? jsonResponse(maslowSection()) : jsonResponse({}, 500));
    const { user } = renderApp("/settings", { fetch: fakeFetch({ "GET /api/settings/maslow": load }) });
    expect(await screen.findByRole("alert")).toHaveTextContent("GET /api/settings/maslow failed with 500");
    healthy = true;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await form()).toBeInTheDocument();
  });

  it("assigns GPIO pins from the action list beside the pin diagram", async () => {
    const { fetch, puts } = settingsServer(gpioSection());
    const { user } = renderApp("/settings/gpio", { fetch });
    const gpio = await screen.findByRole("form", { name: "GPIO Settings" });
    expect(screen.getByRole("img", { name: "Raspberry Pi GPIO pin layout" })).toBeInTheDocument();

    const pin = within(gpio).getByLabelText("GPIO2");
    expect(pin).toHaveValue("");
    expect(
      within(pin)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Unassigned", "Stop", "Pause"]);
    await user.selectOptions(pin, "Stop");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(puts).toEqual([{ gpio2: "Stop" }]));
  });

  it("turns on a WebControl setting from its tab", async () => {
    const { fetch, puts } = settingsServer(webcontrolSection());
    const { user } = renderApp("/settings/webcontrol", { fetch });
    await screen.findByRole("form", { name: "WebControl Settings" });

    const computed = screen.getByLabelText("Computed Sled Position");
    expect(computed).not.toBeChecked();
    expect(computed).toHaveAccessibleDescription(/chain lengths/);
    await user.click(computed);
    await user.click(saveButton());

    await waitFor(() => expect(puts).toEqual([{ computedPosition: true }]));
    expect(screen.getByLabelText("Computed Sled Position")).toBeChecked();
  });

  it("switches sections from the tabs", async () => {
    const advanced: SettingsSection = {
      id: "advanced",
      title: "Advanced Settings",
      enableCustom: false,
      settings: [setting({ key: "openFile", title: "Open File", type: "path", value: "/tmp/a.nc" })],
    };
    const fetch = fakeFetch({
      "GET /api/settings/maslow": () => new Promise<Response>(() => {}),
      "GET /api/settings/advanced": advanced,
    });
    const { user } = renderApp("/settings", { fetch });
    await user.click(screen.getByRole("link", { name: "Advanced" }));
    expect(currentPath()).toBe("/settings/advanced");
    expect(await screen.findByLabelText("Open File")).toHaveValue("/tmp/a.nc");
    expect(screen.getByLabelText("Open File")).toHaveClass("mono");
  });

  it("ignores a load that finishes after leaving the section", async () => {
    let finish: (response: Response) => void = () => {};
    const fetch = fakeFetch({
      "GET /api/settings/maslow": () => new Promise<Response>((resolve) => (finish = resolve)),
      "GET /api/settings/camera": { id: "camera", title: "Camera Settings", enableCustom: false, settings: [] },
    });
    const { user } = renderApp("/settings", { fetch });
    await user.click(screen.getByRole("link", { name: "Camera" }));
    await screen.findByRole("form", { name: "Camera Settings" });
    await act(async () => finish(jsonResponse({}, 500)));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("explains unknown settings addresses", () => {
    renderApp("/settings/nope");
    expect(screen.getByText("There is no settings section at this address.")).toBeInTheDocument();
  });
});
