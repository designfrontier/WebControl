import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";
import { quickConfigureValues } from "./fixtures";

const ROUTE = "/machine/quick-configure";

type PostRoute = NonNullable<Parameters<typeof fakeFetch>[0]>[string];

const renderQuick = (post: PostRoute = quickConfigureValues()) => {
  const fetch = fakeFetch({
    "GET /api/machine/quick-configure": quickConfigureValues(),
    "POST /api/machine/quick-configure": post,
  });
  return { ...renderApp(ROUTE, { fetch }), fetch };
};

const field = (name: string) => screen.getByRole("textbox", { name });
const posted = (fetch: ReturnType<typeof fakeFetch>) =>
  fetch.mock.calls.filter(([, init]) => init?.method === "POST").map(([, init]) => JSON.parse(String(init?.body)));

describe("QuickConfigurePage", () => {
  it("loads the current configuration and saves edits", async () => {
    const { fetch, user } = renderQuick();
    expect(await screen.findByRole("combobox", { name: "Kit" })).toHaveValue("Ring");
    expect(field("Rotation radius (mm)")).toHaveValue("138.4");
    expect(screen.getByRole("combobox", { name: "Chain feeds off the" })).toHaveValue("Top");

    await user.selectOptions(screen.getByRole("combobox", { name: "Kit" }), "Linkage");
    expect(field("Rotation radius (mm)")).toHaveValue("273");
    await user.selectOptions(screen.getByRole("combobox", { name: "Chain feeds off the" }), "Bottom");
    await user.click(screen.getByRole("button", { name: "Subtract 40.4 mm" }));
    expect(field("Distance between motors (mm)")).toHaveValue("2938");
    await user.clear(field("Height of motors above workspace (mm)"));
    await user.type(field("Height of motors above workspace (mm)"), "470");

    await user.click(screen.getByRole("button", { name: "Save configuration" }));
    expect(await screen.findByText("Saved. The controller has the new machine dimensions.")).toBeInTheDocument();
    expect(posted(fetch)).toEqual([
      {
        kinematicsType: "Linkage",
        rotationRadius: 273,
        chainOverSprocket: "Bottom",
        motorSpacingX: 2938,
        motorOffsetY: 470,
      },
    ]);

    await user.type(field("Rotation radius (mm)"), "1");
    expect(screen.queryByText(/^Saved/)).not.toBeInTheDocument();
  });

  it("keeps a custom radius when Custom is picked", async () => {
    const { user } = renderQuick();
    const radius = await screen.findByRole("textbox", { name: "Rotation radius (mm)" });
    await user.clear(radius);
    await user.type(radius, "150");
    await user.selectOptions(screen.getByRole("combobox", { name: "Kit" }), "Custom");
    expect(radius).toHaveValue("150");
  });

  it("flags invalid lengths instead of saving", async () => {
    const { fetch, user } = renderQuick();
    const spacing = await screen.findByRole("textbox", { name: "Distance between motors (mm)" });
    await user.clear(spacing);
    await user.type(spacing, "abc");
    expect(screen.getByRole("button", { name: "Subtract 40.4 mm" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Save configuration" }));
    expect(spacing).toHaveAttribute("aria-invalid", "true");
    expect(spacing).toHaveAccessibleDescription("Enter a length in mm, zero or more.");
    expect(screen.getByText("Fix the highlighted lengths before saving.")).toBeInTheDocument();
    expect(posted(fetch)).toEqual([]);
  });

  it("reports a failed save", async () => {
    const { user } = renderQuick(() => jsonResponse({ error: "chainOverSprocket must be Top or Bottom" }, 400));
    await user.click(await screen.findByRole("button", { name: "Save configuration" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not save: chainOverSprocket must be Top or Bottom",
    );
  });

  it("shows the server's reason when a save is refused during a job", async () => {
    const { user } = renderQuick(() =>
      jsonResponse({ error: "Cannot change settings while a job is running or paused. Stop the job first." }, 409),
    );
    await user.click(await screen.findByRole("button", { name: "Save configuration" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not save: Cannot change settings while a job is running or paused. Stop the job first.",
    );
  });

  it("keeps Save disabled during a job, saying why", async () => {
    const fetch = fakeFetch({
      "GET /api/state": startupState({ jobInProgress: true }),
      "GET /api/machine/quick-configure": quickConfigureValues(),
    });
    renderApp(ROUTE, { fetch });
    expect(await screen.findByText(/Stop the job first to change the settings/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save configuration" })).toBeDisabled();
  });

  it("shows saving while the request is in flight", async () => {
    const { user } = renderQuick(() => new Promise<Response>(() => {}));
    await user.click(await screen.findByRole("button", { name: "Save configuration" }));
    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();
    await user.type(field("Rotation radius (mm)"), "1");
    expect(screen.getByRole("button", { name: "Saving…" })).toBeDisabled();
  });

  it("reports a failed load", async () => {
    renderApp(ROUTE);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the current configuration");
  });
});
