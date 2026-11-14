import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.E2E_BACKEND_PORT ?? 5055);
const python = process.env.E2E_PYTHON ?? "../.venv/bin/python";

/** Runs the real Flask backend (no machine attached, throwaway ~/.WebControl) serving the built app at /next. */
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
    command: `npm run build && ${python} ../tests/e2e_backend.py`,
    url: `http://localhost:${port}/next/`,
    reuseExistingServer: !process.env.CI && !process.env.E2E_BACKEND_PORT,
    timeout: 180_000,
    env: { E2E_BACKEND_PORT: String(port), PYTHONUNBUFFERED: "1" },
  },
});
