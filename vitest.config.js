import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    conditions: process.env.VITEST ? ["browser"] : [],
    alias: {
      // SvelteKit's `$lib` alias normally comes from `sveltekit()`, but
      // this config uses `@sveltejs/vite-plugin-svelte` directly so the
      // page-level component tests (which load `+page.svelte` files
      // that import via `$lib`) can resolve runtime modules.
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
      // `$app/navigation` (SvelteKit runtime) is normally provided by
      // the kit plugin, which we are NOT using in this vitest config.
      // Tests that exercise navigation must inject their own handler;
      // this stub resolves the import to a no-op so component-level
      // specs do not blow up loading the module.
      "$app/navigation": fileURLToPath(
        new URL("./src/test-stubs/app-navigation.ts", import.meta.url),
      ),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.{test,spec}.{js,ts}"],
    globals: true,
  },
});
