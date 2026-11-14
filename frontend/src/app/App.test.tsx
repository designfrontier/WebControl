import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { currentPath, renderApp } from "../test/render";
import { sections } from "./sections";

const nav = () => screen.getByRole("navigation", { name: "Primary" });

describe("App shell", () => {
  it("renders the control screen with the viewer at the index route", () => {
    renderApp("/");
    expect(screen.getByRole("heading", { name: "Control" })).toBeInTheDocument();
    expect(screen.getByRole("figure", { name: "Gcode and board viewer" })).toBeInTheDocument();
  });

  it("has a labelled nav link for every section plus the classic UI", () => {
    renderApp("/");
    const labels = within(nav())
      .getAllByRole("link")
      .map((link) => link.textContent);
    expect(labels).toEqual([...sections.map((section) => section.label), "Classic UI"]);
    expect(within(nav()).getByRole("link", { name: "Classic UI" })).toHaveAttribute("href", "/");
  });

  it.each(sections.filter((section) => section.path !== ""))("navigates to $label", async ({ path, label }) => {
    const { user } = renderApp("/");
    await user.click(within(nav()).getByRole("link", { name: label }));
    expect(currentPath()).toBe(`/${path}`);
    expect(screen.getByRole("heading", { name: label, level: 1 })).toBeInTheDocument();
    expect(within(nav()).getByRole("link", { name: label })).toHaveAttribute("aria-current", "page");
  });

  it("lets a section own nested routes", () => {
    renderApp("/machine/sprockets");
    expect(screen.getByRole("heading", { name: "Machine" })).toBeInTheDocument();
  });

  it("shows a not-found screen for unknown paths", () => {
    renderApp("/nope");
    expect(screen.getByRole("heading", { name: "Not found" })).toBeInTheDocument();
  });

  it("returns home from the brand link", async () => {
    const { user } = renderApp("/settings");
    await user.click(screen.getByRole("link", { name: "WebControl" }));
    expect(currentPath()).toBe("/");
  });

  it("toggles the phone menu and closes it after navigating", async () => {
    const { user } = renderApp("/");
    const toggle = screen.getByRole("button", { name: "Menu" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(nav()).toHaveAttribute("data-open", "true");

    await user.click(within(nav()).getByRole("link", { name: "Files" }));
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });
});
