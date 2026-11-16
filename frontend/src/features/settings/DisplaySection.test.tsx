import { screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";

const theme = () => document.documentElement.getAttribute("data-theme");
const choice = (name: string) =>
  within(screen.getByRole("group", { name: "Display theme" })).getByRole("radio", { name });

/** A server whose PUT /api/display/theme answers only when `answer` is called. */
const themeServer = (theme: "light" | "dark" | "system" = "system", status = 200) => {
  const bodies: unknown[] = [];
  const answers: (() => void)[] = [];
  const fetch = fakeFetch({
    "GET /api/state": startupState({ theme }),
    "PUT /api/display/theme": (init) => {
      const body: unknown = JSON.parse(String(init?.body));
      bodies.push(body);
      return new Promise<Response>((resolve) => answers.push(() => resolve(jsonResponse(body, status))));
    },
  });
  return { fetch, bodies, answerAll: () => answers.forEach((answer) => answer()) };
};

afterEach(() => document.documentElement.removeAttribute("data-theme"));

describe("Display settings", () => {
  it("is its own settings tab with the current theme chosen and no Save button", async () => {
    renderApp("/settings/display", { fetch: themeServer("light").fetch });
    const tabs = within(screen.getByRole("navigation", { name: "Settings sections" }));
    expect(tabs.getByRole("link", { name: "Display" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("heading", { name: "Display", level: 3 })).toBeInTheDocument();
    await waitFor(() => expect(choice("Light")).toBeChecked());
    expect(screen.getByRole("group", { name: "Display theme" })).toHaveAccessibleDescription(/also during a job/);
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });

  it("applies a choice at once, before the server answers, and stores it on the server", async () => {
    const { fetch, bodies, answerAll } = themeServer();
    const { user } = renderApp("/settings/display", { fetch });
    await waitFor(() => expect(choice("System")).toBeChecked());

    await user.click(choice("Dark"));
    expect(theme()).toBe("dark");
    expect(choice("Dark")).toBeChecked();
    expect(bodies).toEqual([{ theme: "dark" }]);
    answerAll();

    await user.click(choice("System"));
    expect(theme()).toBeNull();
    expect(bodies).toEqual([{ theme: "dark" }, { theme: "system" }]);
  });

  it("can be changed while a job runs", async () => {
    const { fetch, bodies } = themeServer("light");
    const { user } = renderApp("/settings/display", {
      fetch,
      machine: { job: { inProgress: true, manualZAxisAdjust: false, probing: false } },
    });
    await waitFor(() => expect(choice("Light")).toBeChecked());
    expect(choice("Dark")).toBeEnabled();
    await user.click(choice("Dark"));
    expect(theme()).toBe("dark");
    expect(bodies).toEqual([{ theme: "dark" }]);
  });

  it("follows a change pushed from another page", async () => {
    const { socket } = renderApp("/settings/display", { fetch: themeServer("light").fetch });
    await waitFor(() => expect(choice("Light")).toBeChecked());
    socket.serverMessage("theme", { theme: "dark" });
    expect(choice("Dark")).toBeChecked();
    expect(theme()).toBe("dark");
  });

  it("goes back to the shown theme and says so when the server refuses", async () => {
    const { fetch, answerAll } = themeServer("light", 400);
    const { user } = renderApp("/settings/display", { fetch });
    await waitFor(() => expect(choice("Light")).toBeChecked());
    await user.click(choice("Dark"));
    expect(theme()).toBe("dark");
    answerAll();
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not save the theme.");
    expect(choice("Light")).toBeChecked();
    expect(theme()).toBe("light");
  });
});
