import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The daemon's port is resolved at runtime (src/daemon.ts) from the `?port=` query parameter the
// Tauri shell hands the window, so nothing here needs to know about the sidecar.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
});
