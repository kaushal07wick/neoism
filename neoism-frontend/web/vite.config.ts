import { defineConfig } from "vite";

export default defineConfig({
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/ws": {
        target: "ws://127.0.0.1:7878/session",
        ws: true,
        changeOrigin: true,
        rewrite: () => "",
      },
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
