import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: "dist",
  },
  resolve: mode === "test" ? { conditions: ["browser"] } : undefined,
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
  },
}));
