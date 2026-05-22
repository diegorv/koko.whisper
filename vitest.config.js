import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

// Vitest config is separate from vite.config.js so the Vite dev/build
// pipeline (which goes through sveltekit()) is not entangled with the
// test runtime. Tests load `$lib/...` directly via the alias below
// because we are NOT loading the SvelteKit plugin here — kit's `$lib`
// alias is normally injected by it, but kit also takes over routing
// and SSR, which is overkill for component unit tests.

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    conditions: process.env.VITEST ? ["browser"] : [],
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.{test,spec}.{js,ts}"],
    globals: true,
  },
});
