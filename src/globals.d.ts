// Build-time defines injected by vite.config.js `define`. The values
// are JSON-stringified at build time so they are inlined as literals
// in the bundle. See `src/lib/utils/build-info.js`.

declare const __BUILD_INFO__: string;
declare const __APP_CHANNEL__: "stable" | "nightly";
