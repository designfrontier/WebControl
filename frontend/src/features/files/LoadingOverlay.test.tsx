import { act, screen, within } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { Dialog } from "../../app/Dialog";
import { topLayer } from "../../test/dialogPolyfill";
import { renderWithProviders } from "../../test/render";
import { LoadingOverlay } from "./LoadingOverlay";

const Harness = () => {
  const [loading, setLoading] = useState(false);
  return (
    <Dialog title="Route modal" onClose={() => {}}>
      <button type="button" onClick={() => setLoading(true)}>
        Load
      </button>
      {loading && <LoadingOverlay title="Loading things…" detail="This takes a moment." />}
      <button type="button" onClick={() => setLoading(false)}>
        Finish
      </button>
    </Dialog>
  );
};

const escapeIsSwallowed = () => {
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  document.body.dispatchEvent(event);
  return event.defaultPrevented;
};

const overlay = () => screen.getByRole<HTMLDialogElement>("dialog", { name: "Loading things…" });

describe("LoadingOverlay", () => {
  const start = async () => {
    const view = renderWithProviders(<Harness />);
    await view.user.click(screen.getByRole("button", { name: "Load" }));
    return view;
  };

  it("opens above the route modal, outside it, with focus and a busy status", async () => {
    await start();
    expect(topLayer()).toHaveLength(2);
    expect(topLayer().at(-1)).toBe(overlay());
    expect(overlay().parentElement).toBe(document.body);
    expect(overlay()).toHaveFocus();
    expect(overlay()).toHaveAttribute("aria-busy", "true");
    expect(overlay()).toHaveAccessibleDescription("This takes a moment.");
    expect(within(overlay()).getByRole("status")).toHaveTextContent("Loading things…This takes a moment.");
  });

  it("ignores Escape and reopens when the browser closes it anyway", async () => {
    const { user } = await start();
    await user.keyboard("{Escape}");
    expect(overlay()).toHaveAttribute("open");
    expect(escapeIsSwallowed()).toBe(true);

    await act(async () => overlay().close());
    expect(overlay()).toHaveAttribute("open");
    expect(topLayer().at(-1)).toBe(overlay());
  });

  it("keeps Stop within reach", async () => {
    const { user, socket } = await start();
    await user.click(within(overlay()).getByRole("button", { name: "Stop machine" }));
    expect(socket.actions()).toContainEqual(["stopRun", "", ""]);
  });

  it("leaves the top layer when unmounted and gives focus back", async () => {
    await start();
    act(() => screen.getByRole("button", { name: "Finish" }).click());
    expect(screen.queryByRole("dialog", { name: "Loading things…" })).not.toBeInTheDocument();
    expect(topLayer()).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Load" })).toHaveFocus();
    expect(escapeIsSwallowed()).toBe(false);
  });
});
