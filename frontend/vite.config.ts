import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const backend = process.env.WEBCONTROL_BACKEND ?? "http://localhost:5000";

export default defineConfig({
  base: "/",
  plugins: [react()],
  server: {
    proxy: {
      "/api": backend,
      "/ws": { target: backend, ws: true },
      "/static": backend,
      // The form and download routes the Files, Boards and System screens use.
      "^/(openGCode|uploadGCode|saveGCode|editGCode|sendGCode|importFile|importFileWCJSON|importRestoreWebControl|downloadDiagnostics|backupWebControl|openBoard|saveBoard|editBoard|trimBoard)$":
        backend,
    },
  },
});
