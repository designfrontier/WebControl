import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.E2E_BACKEND_PORT ?? 5055);
const python = process.env.E2E_PYTHON ?? "../.venv/bin/python";
const rust = process.env.E2E_BACKEND === "rust";

/** Python: `tests/e2e_backend.py`. Rust: `webcontrol-rs` built with this dist embedded, on a throwaway home. */
const backendCommand = rust
  ? `npm run build && cargo build --manifest-path ../rust/Cargo.toml -p wc-server && ` +
    `../rust/target/debug/webcontrol-rs --port ${port} --home "$(mktemp -d)"`
  : `npm run build && ${python} ../tests/e2e_backend.py`;

/** The specs the Rust backend passes so far; each milestone adds its own until it runs them all. */
const rustSpecs = ["rust/**/*.spec.ts", "files-boards.spec.ts", "viewer.spec.ts", "calibration.spec.ts"];

/** Runs a real backend (no machine attached, throwaway ~/.WebControl) serving the built app at /next. */
export default defineConfig({
  testDir: "./e2e",
  ...(rust ? { testMatch: rustSpecs } : { testIgnore: "rust/**" }),
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
    url: `http://localhost:${port}/next/`,
    reuseExistingServer: !process.env.CI && !process.env.E2E_BACKEND_PORT,
    timeout: 180_000,
    env: { E2E_BACKEND_PORT: String(port), PYTHONUNBUFFERED: "1" },
  },
});
