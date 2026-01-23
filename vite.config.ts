import { defineConfig } from "vite";
import { resolve } from "path";
import { analyzer } from "vite-bundle-analyzer";
import react from "@vitejs/plugin-react";
import tsconfigPaths from "vite-tsconfig-paths";
import { sentryVitePlugin } from "@sentry/vite-plugin";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [
    react({
      babel: {
        plugins: [
          "babel-plugin-react-compiler",
          ["@babel/plugin-proposal-decorators", { legacy: true }],
        ],
      },
    }),
    tsconfigPaths(),
    sentryVitePlugin({
      /* @ts-ignore */
      authToken: process.env.SENTRY_AUTH_TOKEN,
      org: "atuin",
      project: "desktop-frontend",
      telemetry: false,
      disable: process.env.CI && process.platform !== 'linux',
      debug: false,
      sourcemaps: {
        // keep sourcemaps for edge builds in the final bundle
        filesToDeleteAfterUpload: process.env.IS_EDGE_BUILD ? [] : ["./dist/assets/*.map"],
      },
    }),
    analyzer({
      analyzerMode: "static",
      enabled: !!process.env.ANALYZE,
    }),
  ],
  build: {
    sourcemap: true,
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        llmtools: resolve(__dirname, 'llmtools.html'),
      },
      output: {
        experimentalMinChunkSize: 100_000,
        manualChunks: {
          'lucide': ['lucide-react'],
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. set port to 1420 but fail over to 1421 if it's in use to support running
  // a second instance with `--config" "backend/tauri-second-instance.conf.json`
  server: {
    hmr: false,
    port: 1420,
    strictPort: false,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  define: {
    "import.meta.vitest": "undefined",
    "process.env.NODE_ENV": JSON.stringify(process.env.NODE_ENV || "development"),
  },
}));
