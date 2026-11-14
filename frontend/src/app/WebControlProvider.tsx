import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import type { Fetch } from "../api/http";
import { getStartupState } from "../api/state";
import type { WebControlClient } from "../socket/client";
import { bindMachineStore } from "../state/bind";
import { createMachineStore } from "../state/machine";
import type { MachineStore } from "../state/machine";
import { ClientContext, FetchContext, MachineContext } from "./context";

interface Props {
  client: WebControlClient;
  fetch: Fetch;
  store?: MachineStore;
  children: ReactNode;
}

export const WebControlProvider = ({ client, fetch, store, children }: Props) => {
  const [machine] = useState(() => store ?? createMachineStore());
  useEffect(() => bindMachineStore(machine, client, () => getStartupState(fetch)), [machine, client, fetch]);
  return (
    <ClientContext value={client}>
      <FetchContext value={fetch}>
        <MachineContext value={machine}>{children}</MachineContext>
      </FetchContext>
    </ClientContext>
  );
};
