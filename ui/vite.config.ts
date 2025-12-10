import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // Base path for production build (embedded in server at /ui/)
  base: "/ui/",
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/v1": {
        target: "http://localhost:8080",
        changeOrigin: true,
      },
      "/ws": {
        target: "ws://localhost:8080",
        ws: true,
      },
    },
    fs: {
      // Allow serving files from the link SDK directory for WASM
      allow: [
        path.resolve(__dirname, "."),
        path.resolve(__dirname, "../link/sdks/typescript"),
      ],
    },
    // Disable caching for WASM and kalam-link files
    headers: {
      "Cache-Control": "no-store",
    },
  },
  build: {
    outDir: "dist",
    sourcemap: true,
    target: "esnext",
  },
  optimizeDeps: {
    // Force re-bundling of kalam-link on every server start
    force: true,
    exclude: ["kalam-link"],
  },
  // Ensure WASM files are handled correctly
  assetsInclude: ["**/*.wasm"],
});
