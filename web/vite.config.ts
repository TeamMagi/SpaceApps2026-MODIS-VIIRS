import { fileURLToPath } from "node:url";
import { defineConfig, searchForWorkspaceRoot } from "vite";
import react from "@vitejs/plugin-react-swc";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    fs: {
      // The Türkiye case study (S7) imports data/case-study/turkey/stats.json from outside web/.
      allow: [searchForWorkspaceRoot(process.cwd()), fileURLToPath(new URL("../data/case-study", import.meta.url))],
    },
    proxy: {
      "/api": {
        target: "http://localhost:3001",
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: "dist",
  },
});
