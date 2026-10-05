// The areas of the frontend suite (`specifications/infra/CIP-ci-pipeline.md`
// CIP-FR-RDCY): each test file is in exactly one area, so the three CI checks
// together run what `pnpm test` runs, and each file once.
//
// The areas come from Vitest itself (`vitest list`), not from a copy of its
// glob rules: Node's glob ignores case on macOS, and Vitest does not.
import { spawnSync } from "node:child_process";
import { existsSync, globSync } from "node:fs";
import { basename, relative, resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

import { ROOT } from "./ciWorkflowModel";
import { TEST_AREAS } from "./testAreas";

type Listed = { file: string; projectName: string };

/** The test files that Vitest finds, each with the area that holds it. */
function vitestList(): Listed[] {
  // The VITEST_* variables of this run would change the child run.
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([key]) => !key.startsWith("VITEST")),
  );
  const result = spawnSync(
    resolve(ROOT, "node_modules/.bin/vitest"),
    ["list", "--filesOnly", "--json"],
    { cwd: ROOT, env, encoding: "utf8" },
  );
  if (result.status !== 0) {
    throw new Error(`vitest list failed: ${result.stderr}`);
  }
  return (JSON.parse(result.stdout) as Listed[]).map((entry) => ({
    file: relative(ROOT, entry.file),
    projectName: entry.projectName,
  }));
}

/**
 * Every file that looks like a test file, in the shapes of Vitest's default
 * include. A file of these shapes that no area holds runs nowhere.
 */
const SKIPPED_DIRS = new Set(["node_modules", ".git", "target", "dist", "coverage"]);
const testLikeFiles = (): string[] =>
  globSync("**/*.{test,spec}.{js,jsx,ts,tsx,cjs,cjsx,mjs,mjsx,cts,ctsx,mts,mtsx}", {
    cwd: ROOT,
    exclude: (path) => SKIPPED_DIRS.has(basename(path)),
  });

describe("CIP-FR-RDCY: the areas of the frontend suite", () => {
  let listed: Listed[] = [];
  const areasOf = (file: string) =>
    listed.filter((entry) => entry.file === file).map((entry) => entry.projectName);

  beforeAll(() => {
    listed = vitestList();
  }, 60_000);

  it("CIP-FR-RDCY: Vitest has one project for each area, and no other project", () => {
    expect([...new Set(listed.map((entry) => entry.projectName))].sort()).toEqual(
      TEST_AREAS.map((area) => area.name).sort(),
    );
  });

  it("CIP-FR-RDCY: puts every test file in exactly one area", () => {
    const files = testLikeFiles();
    expect(files.length).toBeGreaterThan(0);
    const wrong = files
      .map((file) => ({ file, areas: areasOf(file) }))
      .filter(({ areas }) => areas.length !== 1);
    expect(wrong).toEqual([]);
    expect(listed).toHaveLength(files.length);
  });

  // An empty area makes `vitest run --project` find no test file, and the CI
  // check of that area then fails.
  it.each(TEST_AREAS.map((area) => area.name))("CIP-FR-RDCY: %s holds test files", (name) => {
    expect(listed.some((entry) => entry.projectName === name)).toBe(true);
  });

  it.each([
    ["src/components/Editor.find.test.tsx", "editor and documents"],
    ["src/components/PdfViewer/PdfViewer.open.test.tsx", "editor and documents"],
    ["src/components/CommentAttachments.test.ts", "editor and documents"],
    ["src/components/notesGrouping.test.ts", "editor and documents"],
    ["src/components/TopChrome.test.tsx", "components"],
    ["src/components/GraduationRuns/columns.test.tsx", "components"],
    ["src/App.test.tsx", "app and state"],
    ["src/state/editSessions.test.ts", "app and state"],
  ])("CIP-FR-RDCY: puts %s in %s", (file, name) => {
    expect(existsSync(resolve(ROOT, file))).toBe(true);
    expect(areasOf(file)).toEqual([name]);
  });
});
