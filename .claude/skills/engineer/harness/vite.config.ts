/**
 * Vite config for the UI-verification harness.
 *
 * It serves the real frontend from the repo root, but swaps every
 * `@tauri-apps/api/*` import for an in-memory stand-in from this directory, so
 * the app boots and runs in an ordinary browser with no Tauri process behind
 * it. That is what makes the UI drivable by Playwright — the platform WebView
 * the real app renders in is not.
 *
 * Run it from the repo root:
 *   pnpm exec vite --config .claude/skills/engineer/harness/vite.config.ts
 *
 * Port 5199 is deliberately not 1420: the harness must be able to run
 * alongside the developer's own `pnpm tauri dev` without fighting for the port.
 */
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HARNESS = path.dirname(fileURLToPath(import.meta.url));
// .claude/skills/engineer/harness -> repo root
const PROJECT = path.resolve(HARNESS, "../../../..");

export default defineConfig({
  root: PROJECT,
  plugins: [react()],
  resolve: {
    alias: [
      { find: /^@tauri-apps\/api\/core$/, replacement: path.join(HARNESS, "mock-core.ts") },
      { find: /^@tauri-apps\/api\/event$/, replacement: path.join(HARNESS, "mock-event.ts") },
      { find: /^@tauri-apps\/api\/window$/, replacement: path.join(HARNESS, "mock-window.ts") },
      { find: /^@tauri-apps\/plugin-dialog$/, replacement: path.join(HARNESS, "mock-dialog.ts") },
      { find: /^@tauri-apps\/plugin-opener$/, replacement: path.join(HARNESS, "mock-opener.ts") },
    ],
  },
  server: { port: 5199, strictPort: true },
  cacheDir: path.join(HARNESS, ".vite"),
});
