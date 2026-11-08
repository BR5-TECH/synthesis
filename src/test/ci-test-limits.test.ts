/**
 * Conformance tests for the time limits of the test suites
 * (`specifications/infra/CIP-ci-pipeline.md` CIP-FR-EEVU, CIP-FR-VITB,
 * CIP-FR-IGUE).
 *
 * A limit that goes missing fails nothing visibly: the suites stay green until
 * one test waits for ever, and then a lane or an agent turn hangs until an
 * outer timeout stops the whole command. These tests make the loss of a limit
 * a red build.
 */
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import {
  CRATE_DIRS,
  ROOT,
  RUST_LANES,
  Step,
  source,
  stepsOf,
  tracked,
  workflow,
} from "./ciWorkflowModel";

/** The Rust crates whose lanes run `cargo nextest run` (CIP-FR-VITB). */
const NEXTEST_CRATES = ["src-tauri", "tools/agentic-cli-mock", "server"];

const read = (path: string): string => readFileSync(resolve(ROOT, path), "utf8");

/** A text with its `#` comment lines removed. */
const withoutComments = (text: string): string =>
  text
    .split("\n")
    .filter((line) => !line.trim().startsWith("#"))
    .join("\n");

const nextestConfigOf = (crate: string): string =>
  withoutComments(read(`${crate}/.config/nextest.toml`));

/** The text of one TOML table, from its header to the next header. */
const tableOf = (toml: string, header: string): string => {
  const lines = toml.split("\n");
  const start = lines.findIndex((line) => line.trim() === header);
  if (start < 0) return "";
  const end = lines.findIndex((line, i) => i > start && line.trim().startsWith("["));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
};

describe("CIP-FR-VITB nextest stops a test that runs past its limit", () => {
  it("covers every Rust crate the repository holds", () => {
    expect([...CRATE_DIRS].sort()).toEqual([...NEXTEST_CRATES].sort());
  });

  it.each(NEXTEST_CRATES)("%s holds a nextest config", (crate) => {
    expect(existsSync(resolve(ROOT, crate, ".config/nextest.toml"))).toBe(true);
  });

  it.each(NEXTEST_CRATES)(
    "%s marks a test slow after 30 s and stops it after 60 s",
    (crate) => {
      const profile = tableOf(nextestConfigOf(crate), "[profile.default]");
      expect(profile).toMatch(
        /^slow-timeout\s*=\s*\{\s*period\s*=\s*"30s"\s*,\s*terminate-after\s*=\s*2\s*\}\s*$/m,
      );
    },
  );

  it.each(NEXTEST_CRATES)("%s runs the other tests after a failure", (crate) => {
    const profile = tableOf(nextestConfigOf(crate), "[profile.default]");
    expect(profile).toMatch(/^fail-fast\s*=\s*false\s*$/m);
  });

  // An override may give one named test a longer limit. Without
  // `terminate-after`, nextest only reports a slow test and never stops it.
  // Every `slow-timeout` is one inline table, so this check sees each of them.
  it.each(NEXTEST_CRATES)("%s has no override that removes the limit", (crate) => {
    const config = nextestConfigOf(crate);
    expect(config, "a slow-timeout in a dotted key or a sub-table").not.toMatch(
      /slow-timeout\s*\.|\[[^\]]*slow-timeout[^\]]*\]/,
    );
    expect(config, "on-timeout can pass a test that timed out").not.toMatch(/on-timeout/);
    for (const value of config.matchAll(/^\s*slow-timeout\s*=\s*(.+)$/gm)) {
      expect(value[1], `slow-timeout without a stop: ${value[0]}`).toMatch(
        /terminate-after\s*=\s*[1-9]\d*/,
      );
    }
  });

  // A profile or a config file chosen on the command line, or fail-fast turned
  // on, would replace the limits above for that run.
  it.each(RUST_LANES)("%s runs nextest with the default profile and config", (lane) => {
    for (const step of stepsOf(lane)) {
      expect(withoutComments(step.run ?? "")).not.toMatch(
        /--profile|--config-file|--fail-fast|--no-tests=pass/,
      );
    }
  });

  it("sets no NEXTEST_ variable in the workflow", () => {
    const envs: Record<string, unknown>[] = [workflow.env ?? {}];
    for (const job of Object.values(workflow.jobs) as { env?: object; steps?: Step[] }[]) {
      envs.push((job.env ?? {}) as Record<string, unknown>);
      for (const step of job.steps ?? []) envs.push(step.env ?? {});
    }
    for (const env of envs) {
      expect(Object.keys(env).filter((name) => name.startsWith("NEXTEST_"))).toEqual([]);
    }
  });
});

describe("CIP-FR-IGUE every Rust lane installs one cargo-nextest version", () => {
  const version = String(workflow.env?.CARGO_NEXTEST_VERSION ?? "");
  const INSTALLS_THE_PINNED_VERSION =
    /^cargo-nextest@\$\{\{\s*env\.CARGO_NEXTEST_VERSION\s*\}\}$/;

  it("names one exact version", () => {
    expect(version).toMatch(/^\d+\.\d+\.\d+$/);
  });

  // A job or a step that set the variable again would install a different
  // version in that lane.
  it("names the version once, for the whole workflow", () => {
    expect(source.match(/^\s*CARGO_NEXTEST_VERSION\s*:/gm)).toHaveLength(1);
  });

  it("is the version the agent images carry (AVI-FR-PBVT)", () => {
    for (const vendor of ["claude-code", "codex"]) {
      const dockerfile = read(`docker/agent-images/${vendor}/Dockerfile`);
      expect(dockerfile).toMatch(new RegExp(`^ARG NEXTEST_VERSION=${version}$`, "m"));
    }
  });

  it("has the three Rust lanes", () => {
    expect([...RUST_LANES].sort()).toEqual(["backend", "mock", "server"]);
  });

  it.each(RUST_LANES)(
    "%s installs the prebuilt release at that version before its test step",
    (lane) => {
      const steps = stepsOf(lane);
      const install = steps.findIndex(
        (step) =>
          step.uses?.startsWith("taiki-e/install-action@") &&
          INSTALLS_THE_PINNED_VERSION.test(String(step.with?.tool)),
      );
      const test = steps.findIndex((step) => /\bcargo nextest run\b/.test(step.run ?? ""));
      expect(install, `${lane} does not install cargo-nextest`).toBeGreaterThanOrEqual(0);
      expect(test, `${lane} does not run cargo nextest run`).toBeGreaterThan(install);
    },
  );

  // `cargo test` has no limit for one test, so a lane that kept it would hang
  // on a test that waits for ever.
  it.each(RUST_LANES)("%s runs no cargo test", (lane) => {
    for (const step of stepsOf(lane)) {
      expect(withoutComments(step.run ?? "")).not.toMatch(/\bcargo test\b/);
    }
  });
});

describe("CIP-FR-EEVU Vitest runs every test and hook under a limit", () => {
  const config = read("vitest.config.ts");

  it.each([
    ["testTimeout", 5_000],
    ["hookTimeout", 10_000],
    ["teardownTimeout", 10_000],
  ])("sets %s to %i ms", (name, ms) => {
    const value = config.match(new RegExp(`\\b${name}:\\s*([\\d_]+)`))?.[1];
    expect(value, `${name} is not set`).toBeDefined();
    expect(Number(value!.replace(/_/g, ""))).toBe(ms);
  });

  // A project of an area that set its own limit would replace the one above
  // for every test in that area (CIP-FR-RDCY).
  it("lets no area project replace the limits", () => {
    expect(config.match(/\b(testTimeout|hookTimeout|teardownTimeout):/g)).toHaveLength(3);
  });

  // Vitest reads a limit of 0 or less as no limit at all.
  it("lets no test remove its limit", () => {
    const files = [
      ...(tracked("src/**/*.test.ts") ?? []),
      ...(tracked("src/**/*.test.tsx") ?? []),
    ];
    expect(files.length).toBeGreaterThan(100);
    const removal =
      /vi\.setConfig|\b(testTimeout|hookTimeout|teardownTimeout)\b|timeout:\s*(0|-\d|Infinity)\b|^\s*\},\s*(0|-\d+|Infinity)\s*\);?\s*$/m;
    const offenders = files.filter(
      (file) => file !== "src/test/ci-test-limits.test.ts" && removal.test(read(file)),
    );
    expect(offenders).toEqual([]);
  });

  it("is not replaced on the command line", () => {
    const flag = /--(test|hook|teardown)Timeout\b|--(test|hook|teardown)-timeout\b/;
    expect(withoutComments(source)).not.toMatch(flag);
    expect(read("package.json")).not.toMatch(flag);
    expect(withoutComments(read("Taskfile.yaml"))).not.toMatch(flag);
  });
});
