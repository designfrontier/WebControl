import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { App } from "./app/App";
import { applyPreference, readPreference } from "./app/themePreference";
import { WebControlProvider } from "./app/WebControlProvider";
import { createClient } from "./socket/client";
import { connectSocket } from "./socket/connect";
import { selectTransport, watchBackend } from "./transport";
import "./styles/theme.css";
import "./styles/app.css";

applyPreference(readPreference());

const browserFetch = (input: string, init?: RequestInit) => fetch(input, init);

void selectTransport(browserFetch).then((backend) => {
  const socket = connectSocket();
  watchBackend(socket, backend, { fetch: browserFetch, reload: () => window.location.reload() });
  const client = createClient(socket);

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <BrowserRouter basename="/next">
        <WebControlProvider client={client} fetch={browserFetch}>
          <App />
        </WebControlProvider>
      </BrowserRouter>
    </StrictMode>,
  );
});
