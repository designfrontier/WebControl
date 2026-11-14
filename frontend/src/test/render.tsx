import { render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode } from "react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { App } from "../app/App";
import { WebControlProvider } from "../app/WebControlProvider";
import type { Fetch } from "../api/http";
import { createClient } from "../socket/client";
import { createMachineStore, initialMachineState } from "../state/machine";
import type { MachineState } from "../state/machine";
import { fakeFetch } from "./fakeFetch";
import { createFakeSocket } from "./fakeSocket";
import type { FakeSocket } from "./fakeSocket";
import { Where } from "./Where";

export interface RenderOptions {
  route?: string;
  socket?: FakeSocket;
  fetch?: Fetch;
  /** Seeds the machine store, e.g. `{ status: { uploadFlag: 1, ... } }`. */
  machine?: Partial<MachineState>;
  /** Connects the fake socket after mounting, as the real server would. Defaults to true. */
  connect?: boolean;
}

/** Renders `ui` inside every app provider with a fake socket, fake fetch and a memory router. */
export const renderWithProviders = (
  ui: ReactNode,
  { route = "/", socket = createFakeSocket(), fetch = fakeFetch(), machine = {}, connect = true }: RenderOptions = {},
) => {
  const client = createClient(socket);
  const store = createMachineStore({ ...initialMachineState, ...machine });
  const user = userEvent.setup();
  const result = render(
    <StrictMode>
      <MemoryRouter initialEntries={[route]}>
        <WebControlProvider client={client} fetch={fetch} store={store}>
          {ui}
          <Where />
        </WebControlProvider>
      </MemoryRouter>
    </StrictMode>,
  );
  if (connect) socket.serverConnect();
  return { ...result, socket, client, store, fetch, user };
};

/** Renders the whole app at `route`. */
export const renderApp = (route = "/", options: Omit<RenderOptions, "route"> = {}) =>
  renderWithProviders(<App />, { ...options, route });

export const currentPath = () => document.querySelector('[data-testid="location"]')?.textContent;
