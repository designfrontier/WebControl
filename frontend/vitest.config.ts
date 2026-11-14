import { configDefaults, defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    // Contract tests read the Python sources (Actions/actions.py, static/scripts/baseSocket.js) with ?raw imports.
    server: { fs: { allow: [".."] } },
    test: {
      environment: "jsdom",
      setupFiles: ["./src/test/setup.ts"],
      css: false,
      exclude: [...configDefaults.exclude, "e2e/**"],
      coverage: {
        provider: "v8",
        reporter: ["text-summary", "lcov"],
        include: ["src/**/*.{ts,tsx}"],
        exclude: ["src/**/*.test.{ts,tsx}", "src/**/index.ts", "src/**/*.d.ts", "src/test/**", "src/main.tsx"],
        thresholds: { lines: 95, branches: 95, functions: 95, statements: 95 },
      },
    },
  }),
);
