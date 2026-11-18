import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const backend = process.env.WEBCONTROL_BACKEND ?? "http://localhost:5000";
// The string shorthand rewrites Host to the backend's, which then refuses the page's Origin as another site's.
const proxy = { target: backend, changeOrigin: false };

export default defineConfig({
  base: "/",
  plugins: [react()],
  server: {
    proxy: {
      "/api": proxy,
      "/ws": { ...proxy, ws: true },
      "/static": proxy,
      // The form and download routes the Files, Boards and System screens use.
      "^/(openGCode|uploadGCode|saveGCode|editGCode|sendGCode|importFile|importFileWCJSON|importRestoreWebControl|downloadDiagnostics|backupWebControl|openBoard|saveBoard|editBoard|trimBoard)$":
        proxy,
    },
  },
});
