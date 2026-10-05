import { configDefaults, defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

import { TEST_AREAS } from "./src/test/testAreas";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    // Node 25 and later puts its own `localStorage` and `sessionStorage` on the
    // global. Without `--localstorage-file`, they are `undefined`. Vitest does
    // not replace a global that already exists, so the tests do not get the
    // jsdom storage. This flag removes the Node storage from the test workers.
    execArgv: ["--no-experimental-webstorage"],
    // CIP-FR-RDCY: one project for each area of the suite. `pnpm test` runs
    // all of them; CI runs each one as its own check.
    projects: TEST_AREAS.map((area) => ({
      extends: true,
      test: {
        name: area.name,
        include: area.include,
        exclude: [...configDefaults.exclude, ...area.exclude],
      },
    })),
  },
});
