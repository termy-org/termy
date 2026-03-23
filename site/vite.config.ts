import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import path from "path";

export default defineConfig({
  plugins: [
    tanstackRouter({
      quoteStyle: "double",
      target: "react",
      autoCodeSplitting: true,
    }),
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    proxy: {
      "/theme-api": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
        rewrite: (routePath) => routePath.replace(/^\/theme-api/, ""),
      },
      "/api": "http://localhost:3000",
    },
  },
});
