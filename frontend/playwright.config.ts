import { defineConfig, devices } from "@playwright/test";
import { simulatorPort } from "./e2e/simulator";

const port = Number(process.env.E2E_BACKEND_PORT ?? 5055);
const binary = `"\${CARGO_TARGET_DIR:-../rust/target}/debug/webcontrol-rs"`;

/** `webcontrol-rs` built with this dist embedded, on a throwaway home and with no controller. */
const backendCommand =
  `npm run build && cargo build --manifest-path ../rust/Cargo.toml -p wc-server && ` +
  `${binary} --port ${port} --home "$(mktemp -d)"`;

export default defineConfig({
  testDir: "./e2e",
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"]],
  use: { baseURL: `http://localhost:${port}`, trace: "retain-on-failure" },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"] } },
    { name: "phone", use: { ...devices["Pixel 7"] } },
  ],
  // Started in order, so the second reuses the binary the first built.
  webServer: [
    {
      command: backendCommand,
      url: `http://localhost:${port}/`,
      reuseExistingServer: !process.env.CI && !process.env.E2E_BACKEND_PORT,
      // A CI run whose Cargo.lock changed rebuilds the server from a cold cache before it can answer.
      timeout: process.env.CI ? 600_000 : 180_000,
      env: { E2E_BACKEND_PORT: String(port) },
    },
    {
      command: `${binary} --port ${simulatorPort} --home "$(mktemp -d)" --simulate`,
      url: `http://localhost:${simulatorPort}/`,
      reuseExistingServer: false,
      timeout: 60_000,
    },
  ],
});
