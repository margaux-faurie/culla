/// <reference types="vitest/config" />
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// ADR 0014: the backend is chosen at build time. `--mode desktop` links the Tauri backend;
// every other mode (demo, test) links the demo backend, so @tauri-apps/* never reaches the demo.
const backendFor = (mode: string): string => {
  const file = mode === "desktop" ? "./src/backend/tauri.ts" : "./src/backend/demo.ts";
  return fileURLToPath(new URL(file, import.meta.url));
};

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  resolve: {
    alias: {
      "#backend-impl": backendFor(mode),
    },
  },
  // Tauri reads the dev server output and expects a fixed port (src-tauri/tauri.conf.json devUrl).
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: mode === "demo" ? "dist-demo" : "dist",
    emptyOutDir: true,
  },
  test: {
    include: ["src/**/*.test.{ts,tsx}", "tests/**/*.test.ts"],
    environment: "node",
  },
}));
