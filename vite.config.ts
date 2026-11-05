import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Интерфейс лежит в ui/, собирается в dist/ — его подхватывает Tauri.
export default defineConfig({
  root: "ui",
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "../dist", emptyOutDir: true, target: "es2022", chunkSizeWarningLimit: 800 },
});
