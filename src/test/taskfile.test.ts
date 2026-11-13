/**
 * Conformance tests for the verification tasks of `Taskfile.yaml`
 * (`specifications/infra/TSK-taskfile.md` TSK-FR-24, TSK-FR-25).
 *
 * An agent runs the project checks through these tasks. A task that went back
 * to `cargo test` would hang on a test that waits for ever, while CI stays
 * green, because CI does not run the tasks.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { parse } from "yaml";

type Cmd = string | { cmd?: string; task?: string };
type Task = { dir?: string; cmds?: Cmd[] };

const taskfile = parse(readFileSync(resolve(process.cwd(), "Taskfile.yaml"), "utf8")) as {
  tasks: Record<string, Task>;
};

/** The shell lines of a task's commands, without comments, `echo`, or `trap`. */
const linesOf = (task: Task): string[] =>
  (task.cmds ?? [])
    .flatMap((cmd) => (typeof cmd === "string" ? cmd : (cmd.cmd ?? "")).split("\n"))
    .map((line) => line.trim())
    .filter((line) => line && !/^(#|echo |trap )/.test(line));

/** The lines that follow `cd '<dir>'`, up to the next `cd`. */
const blockAfterCd = (lines: string[], dirVar: string): string[] => {
  const start = lines.findIndex((line) => line.includes(`{{.${dirVar}}}`));
  if (start < 0) return [];
  const end = lines.findIndex((line, i) => i > start && line.startsWith("cd "));
  return lines.slice(start + 1, end < 0 ? undefined : end);
};

describe("TSK-FR-24 tests:backend runs the backend lane", () => {
  const task = taskfile.tasks["tests:backend"];

  it("runs from src-tauri/", () => {
    expect(task.dir).toMatch(/\/src-tauri$/);
  });

  it("runs lint:backend, then cargo check, then cargo nextest run", () => {
    const cmds = task.cmds ?? [];
    const lint = cmds.findIndex((cmd) => typeof cmd === "object" && cmd.task === "lint:backend");
    const check = cmds.indexOf("cargo check");
    const test = cmds.indexOf("cargo nextest run");
    expect(lint).toBeGreaterThanOrEqual(0);
    expect(check).toBeGreaterThan(lint);
    expect(test).toBeGreaterThan(check);
  });
});

describe("TSK-FR-25 tests runs every lane", () => {
  const lines = linesOf(taskfile.tasks.tests);

  it("runs cargo check, then cargo nextest run, in the mock crate", () => {
    expect(blockAfterCd(lines, "MOCK_LANE_DIR")).toEqual(["cargo check", "cargo nextest run"]);
  });

  it("runs cargo build, then cargo nextest run, with --locked in the server crate", () => {
    expect(blockAfterCd(lines, "SERVER_LANE_DIR")).toEqual([
      "cargo build --locked",
      "cargo nextest run --locked",
    ]);
  });
});

// `cargo test` has no limit for one test (CIP-FR-VITB). `uat:dind`,
// `uat:openspec` and `uat:openspec-image` keep it on purpose: they are manual
// acceptance tests and are not part of `tests`.
describe("TSK-FR-24, TSK-FR-25 no verification task runs cargo test", () => {
  it.each(["tests", "tests:backend"])("%s runs no cargo test", (name) => {
    expect(linesOf(taskfile.tasks[name]).filter((line) => /\bcargo test\b/.test(line))).toEqual(
      [],
    );
  });
});

describe("TSK-FR-01 the task file holds the fifteen tasks", () => {
  it("names each task and no other", () => {
    expect(Object.keys(taskfile.tasks).sort()).toEqual(
      [
        "build",
        "build:linux",
        "build:macos",
        "build:windows",
        "docker",
        "lint",
        "lint:backend",
        "lint:mock",
        "lint:server",
        "specs",
        "tests",
        "tests:backend",
        "uat:dind",
        "uat:openspec",
        "uat:openspec-image",
      ].sort(),
    );
  });
});

describe("TSK-FR-NJQE, TSK-FR-HEXG the OpenSpec acceptance tasks", () => {
  it.each([
    ["uat:openspec", "openspec_uat_cli"],
    ["uat:openspec-image", "openspec_uat_image"],
  ])("%s runs the ignored %s tests from src-tauri/, one thread at a time", (name, filter) => {
    const task = taskfile.tasks[name];
    expect(task.dir).toBe("{{.ROOT_DIR}}/src-tauri");
    expect(linesOf(task)).toEqual([
      `TMPDIR=/tmp cargo test --lib -- --ignored --test-threads=1 --nocapture ${filter}`,
    ]);
  });

  it("no verification task runs an OpenSpec acceptance test", () => {
    for (const name of ["tests", "tests:backend"]) {
      expect(JSON.stringify(taskfile.tasks[name])).not.toMatch(/openspec_uat|uat:openspec/);
    }
  });
});
