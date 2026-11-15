import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.E2E_BACKEND_PORT ?? 5055);

/** `webcontrol-rs` built with this dist embedded, on a throwaway home and with no controller. */
const backendCommand =
  `npm run build && cargo build --manifest-path ../rust/Cargo.toml -p wc-server && ` +
  `"\${CARGO_TARGET_DIR:-../rust/target}/debug/webcontrol-rs" --port ${port} --home "$(mktemp -d)"`;

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
  webServer: {
    command: backendCommand,
    url: `http://localhost:${port}/`,
    reuseExistingServer: !process.env.CI && !process.env.E2E_BACKEND_PORT,
    timeout: 180_000,
    env: { E2E_BACKEND_PORT: String(port) },
  },
});
