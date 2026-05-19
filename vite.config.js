// @ts-nocheck
import { readFileSync } from "fs";
import { execSync } from "child_process";
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { formatBuildInfo, parseReleaseChannel } from "./src/lib/utils/build-info.js";

// Build-info: computed once at config-load time. Stable builds get the
// pkg version + short sha; nightly builds get the synthesised
// `X.Y.Z-nightly.<count>.<sha>` so the auto-updater's semver comparator
// can pick "newest" correctly.

const pkg = JSON.parse(readFileSync("./package.json", "utf-8"));

let gitHash = "unknown";
try { gitHash = execSync("git rev-parse --short HEAD").toString().trim(); } catch {}

let commitCount = "0";
try { commitCount = execSync("git rev-list --count HEAD").toString().trim(); } catch {}

const now = new Date();
const pad = (n) => String(n).padStart(2, "0");
const buildTime = [
  `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`,
  `T${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`,
].join("");

const channel = parseReleaseChannel(process.env.KOKO_RELEASE_CHANNEL);
const buildInfo = formatBuildInfo({ pkgVersion: pkg.version, gitHash, commitCount, buildTime, channel });

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [sveltekit()],
  clearScreen: false,
  define: {
    __BUILD_INFO__: JSON.stringify(buildInfo),
    __APP_CHANNEL__: JSON.stringify(channel),
  },
  server: {
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
      ignored: ["**/src-tauri/**"],
    },
  },
}));
