import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { resolve } from "node:path";

// Multi-page build: the HUD, the Una window and the correction popup are
// separate pages, created by the tauri layer as separate WebviewWindows.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "safari15",
    rollupOptions: {
      input: {
        hud: resolve(__dirname, "hud.html"),
        app: resolve(__dirname, "app.html"),
        correction: resolve(__dirname, "correction.html"),
      },
    },
  },
});
