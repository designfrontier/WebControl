import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fakeFetch, jsonResponse, startupState } from "../../test/fakeFetch";
import { renderApp } from "../../test/render";

const slot = (name: string) =>
  within(screen.getByRole("group", { name: "Control buttons" })).getByRole("combobox", { name });

const server = (status = 200) => {
  const bodies: unknown[] = [];
  const fetch = fakeFetch({
    "GET /api/state": startupState({
      macros: [
        { command: "macro1", title: "Spindle on", empty: false },
        { command: "macro2", title: "Macro 2", empty: true },
        { command: "macro3", title: "", empty: true },
        { command: "macro4", title: "Vacuum", empty: false },
      ],
    }),
    "PUT /api/display/controls": (init) => {
      const body: unknown = JSON.parse(String(init?.body));
      bodies.push(body);
      return jsonResponse(status === 200 ? body : { error: "refused" }, status);
    },
  });
  return { fetch, bodies };
};

describe("Control buttons settings", () => {
  it("shows what each slot holds, with every choice and each macro by its title", async () => {
    renderApp("/settings/display", { fetch: server().fetch });
    expect(slot("Right column, top")).toHaveValue("park");
    expect(slot("Right column, middle")).toHaveValue("zAxis");
    expect(slot("Right column, bottom")).toHaveValue("setHome");
    expect(slot("Beside the distance")).toHaveValue("setPark");
    await waitFor(() =>
      expect(
        within(slot("Right column, top"))
          .getAllByRole("option")
          .map((option) => option.textContent),
      ).toEqual([
        "Park",
        "Z-Axis",
        "Set Home Here",
        "Set Park Here",
        "Macro 1: Spindle on",
        "Macro 2",
        "Macro 3",
        "Macro 4: Vacuum",
        "Go to Z0",
        "Go to Safe Height",
      ]),
    );
    expect(screen.getByRole("group", { name: "Control buttons" })).toHaveAccessibleDescription(/swaps the two/);
  });

  it("stores a choice at once, swapping a button another slot held, and the grid follows", async () => {
    const { fetch, bodies } = server();
    const { user, store } = renderApp("/settings/display", { fetch });
    await user.selectOptions(slot("Right column, top"), "macro4");
    expect(bodies).toEqual([{ buttons: ["macro4", "zAxis", "setHome", "setPark"] }]);
    await user.selectOptions(slot("Beside the distance"), "zAxis");
    expect(bodies.at(-1)).toEqual({ buttons: ["macro4", "setPark", "setHome", "zAxis"] });
    expect(slot("Right column, middle")).toHaveValue("setPark");
    expect(store.getState().controlButtons).toEqual(["macro4", "setPark", "setHome", "zAxis"]);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("puts the slots back and says so when the server refuses", async () => {
    const { user } = renderApp("/settings/display", { fetch: server(400).fetch });
    await user.selectOptions(slot("Right column, bottom"), "macro1");
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not save the control buttons.");
    expect(slot("Right column, bottom")).toHaveValue("setHome");
  });
});
