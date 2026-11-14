import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch } from "../../test/fakeFetch";
import { currentPath, renderApp } from "../../test/render";
import { machineSetup, quickConfigureValues } from "./fixtures";

const fetch = () =>
  fakeFetch({ "/api/machine/setup": machineSetup(), "/api/machine/quick-configure": quickConfigureValues() });

describe("MachinePage", () => {
  it("lists every setup screen and opens each one", async () => {
    const { user } = renderApp("/machine", { fetch: fetch() });
    expect(screen.getByRole("heading", { level: 1, name: "Machine" })).toBeInTheDocument();
    const screens = [
      "Getting started",
      "Quick configure",
      "Sprockets and chains",
      "Reset chains",
      "Z-axis",
      "Z-axis limits",
      "Fake servo",
    ];
    await screens.reduce(async (previous, title) => {
      await previous;
      await user.click(screen.getByRole("link", { name: title }));
      expect(screen.getByRole("heading", { level: 2, name: title })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 1, name: "Machine" })).toBeInTheDocument();
      await user.click(screen.getByRole("link", { name: "← All machine setup" }));
      expect(currentPath()).toBe("/machine");
    }, Promise.resolve());
  });

  it("sends unknown machine paths back to the list", () => {
    renderApp("/machine/nope");
    expect(currentPath()).toBe("/machine");
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
