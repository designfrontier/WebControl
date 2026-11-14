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
    },
  },
});
