import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Port must match devUrl in ../tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
});
