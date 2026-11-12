import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
export default defineConfig({
  plugins: [vue()],
  build: {
    rollupOptions: { input: { main: "index.html", tray: "tray.html" } },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/**",
        "**/vendor/**",
        "**/artifacts/**",
        "**/.local/**",
        "**/plugins/**/obj/**",
        "**/plugins/**/bin/**",
      ],
    },
  },
  clearScreen: false,
});
