import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { machineSetup, quickConfigureValues } from "./fixtures";

const fetch = () =>
  fakeFetch({ "/api/machine/setup": machineSetup(), "/api/machine/quick-configure": quickConfigureValues() });

describe("machine setup modals", () => {
  it.each([
    ["/machine/getting-started", "Getting Started"],
    ["/machine/quick-configure", "Quick Configure"],
    ["/machine/sprockets", "Set Sprockets & Chains"],
    ["/machine/reset-chains", "Reset Chains"],
    ["/machine/z-axis", "Set Z-Axis"],
    ["/machine/z-limits", "Set Z-Axis Limits"],
    ["/machine/fake-servo", "Fake Servo"],
  ])("opens %s as the %s modal over the control screen", (path, title) => {
    renderApp(path, { fetch: fetch() });
    expect(screen.getByRole("dialog", { name: title })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "Control", hidden: true })).toBeInTheDocument();
  });

  it("sends the old machine list to the Actions modal, which links every setup screen", async () => {
    const { user } = renderApp("/machine", { fetch: fetch() });
    expect(currentPath()).toBe("/actions");
    await user.click(screen.getByRole("link", { name: "Set sprockets and chains" }));
    expect(currentPath()).toBe("/machine/sprockets");
    expect(screen.getByRole("dialog", { name: "Set Sprockets & Chains" })).toBeInTheDocument();
  });

  it("explains an unknown machine path", () => {
    renderApp("/machine/nope");
    expect(screen.getByRole("dialog", { name: "Not found" })).toBeInTheDocument();
  });

  it("links the getting started steps to the screens that do them", () => {
    renderApp("/machine/getting-started", { fetch: fetch() });
    expect(screen.getByRole("link", { name: "Quick configure" })).toHaveAttribute("href", "/machine/quick-configure");
    expect(screen.getByRole("link", { name: "Sprockets and chains" })).toHaveAttribute("href", "/machine/sprockets");
    expect(screen.getByRole("link", { name: "reset the chains" })).toHaveAttribute("href", "/machine/reset-chains");
    expect(screen.getByRole("img", { name: "Sprocket with a vertical tooth" })).toHaveAttribute(
      "src",
      "/static/images/gettingStarted/Sprocket%20at%2012-00.png",
    );
  });
});
