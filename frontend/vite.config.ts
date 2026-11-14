import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const backend = process.env.WEBCONTROL_BACKEND ?? "http://localhost:5000";

export default defineConfig({
  base: "/next/",
  plugins: [react()],
  server: {
    proxy: {
      "/api": backend,
      "/socket.io": { target: backend, ws: true },
      "/static": backend,
      // Classic form and download routes the Files and Boards screens reuse.
      "^/(openGCode|uploadGCode|saveGCode|editGCode|sendGCode|importFile|importFileWCJSON|importRestoreWebControl|downloadDiagnostics|backupWebControl|openBoard|saveBoard|editBoard|trimBoard)$":
        backend,
    },
  },
});
