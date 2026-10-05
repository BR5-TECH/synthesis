/**
 * What the repository's own ignore rules may and may not hide.
 *
 * An unanchored pattern matches a directory of that name at **any** depth. A
 * rule meant for one build directory at the root therefore hides authored
 * source anywhere in the tree, and what it hides is absent from a graduation
 * run's change set: nobody reviews it, and the run region can only say how many
 * such paths there were rather than what they held
 * (`specifications/ui/GRU-graduation-runs.md` GRU-FR-WJHV). The failure is
 * silent, which is why it is asserted here rather than left to the next run
 * that trips over it.
 */
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

/**
 * Whether the repository's ignore rules exclude a path, asked of git itself so
 * the answer is the one every walk gets. `--no-index` keeps the question about
 * the rules rather than about what happens to be tracked today.
 *
 * `undefined` where there is no git to ask — a source tarball, or a build
 * context copied without `.git` — on the terms `ci-workflow.test.ts` already
 * sets for that case.
 */
const ignored = (path: string): boolean | undefined => {
  try {
    execFileSync("git", ["check-ignore", "-q", "--no-index", "--", path], {
      cwd: ROOT,
      stdio: "ignore",
    });
    return true;
  } catch (error) {
    // Exit 1 is "not ignored", which is an answer. Anything else is no git.
    const status = (error as { status?: number }).status;
    return status === 1 ? false : undefined;
  }
};

describe("the repository's ignore rules", () => {
  it("GRU-FR-WJHV: hide no source directory whose name matches a build-output rule", () => {
    for (const path of [
      // The three build-output patterns that were unanchored, each tested at a
      // depth a source directory of that name really could sit at. `node_modules`
      // is deliberately still unanchored: a nested one is real, and the
      // dependency floor of GRU-FR-WJHV covers it whatever the rules say.
      "src-tauri/src/graduation/logs/records.rs",
      "src/components/dist/index.ts",
      "src/state/dist-ssr/state.ts",
    ]) {
      const hidden = ignored(path);
      if (hidden === undefined) return;
      expect(hidden, `${path} is authored source and must reach a change set`).toBe(false);
    }
  });

  it("GRU-FR-WJHV: still hide the build output those rules are for", () => {
    // Each probe is a file only its **own** rule matches. `logs/app.log` would
    // not do: `*.log` matches it, so it stays ignored even with the `/logs/`
    // rule deleted, and the assertion would guard nothing.
    for (const path of ["logs/keep.txt", "dist/index.js", "dist-ssr/entry.js"]) {
      const hidden = ignored(path);
      if (hidden === undefined) return;
      expect(hidden, `${path} is build output and stays out of a change set`).toBe(true);
    }
  });
});
