import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

export default defineConfig({
  plugins: [svelte({ hot: !process.env.VITEST })],
  resolve: {
    conditions: process.env.VITEST ? ["browser"] : [],
    alias: {
      // SvelteKit's `$lib` alias normally comes from `sveltekit()`, but
      // this config uses `@sveltejs/vite-plugin-svelte` directly so the
      // page-level component tests (which load `+page.svelte` files
      // that import via `$lib`) can resolve runtime modules.
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.{test,spec}.{js,ts}"],
    globals: true,
  },
});
