// @ts-nocheck
import { readFileSync } from "fs";
import { execSync } from "child_process";
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

// Build-info: computed once at config-load time and injected into the
// Svelte bundle via a `define` constant. The Settings page renders
// this string so the user (and bug reports) can identify exactly
// which build is installed without shipping channel UX or an
// auto-updater (deliberate scope cut — see ADR-0001).

const pkg = JSON.parse(readFileSync("./package.json", "utf-8"));

let gitHash = "unknown";
try {
  gitHash = execSync("git rev-parse --short HEAD").toString().trim();
} catch {}

const buildInfo = `${pkg.version} (${gitHash})`;

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [sveltekit()],
  clearScreen: false,
  define: {
    __BUILD_INFO__: JSON.stringify(buildInfo),
  },
  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  server: {
    // tauri expects a fixed port, fail if that port is not available
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
